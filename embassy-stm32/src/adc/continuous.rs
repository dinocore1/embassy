use core::marker::PhantomData;

use super::*;
use crate::dma::ReadableRingBuffer;
use crate::time::Hertz;
use crate::{interrupt, Peripheral};
use embassy_hal_internal::{into_ref, PeripheralRef};
use embedded_hal_02::blocking::delay::DelayUs;
use crate::interrupt::typelevel::Interrupt;
use crate::timer::sealed::GeneralPurpose16bitInstance as BasicTimer;

/// Interrupt handler.
pub struct InterruptHandler<T: Instance> {
    _phantom: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        info!("on_interrupt");
        if T::regs().isr().read().eoc() {
            //T::regs().ier().modify(|w| w.set_eocie(false));
        } else {
            return;
        }

        T::state().waker.wake();
    }
}

pub struct ContinuousAdc<'d, T, Timer, AdcDma>
where T: super::Instance,
    Timer: BasicTimer,
    AdcDma: super::AdcDma<T>,
{
    #[allow(unused)]
    adc: PeripheralRef<'d, T>,
    timer: PeripheralRef<'d, Timer>,
    dma_ch: PeripheralRef<'d, AdcDma>,
}

impl<'d, T, Timer, AdcDma> ContinuousAdc<'d, T, Timer, AdcDma>
where T: super::Instance,
    Timer: BasicTimer,
    AdcDma: super::AdcDma<T>,
{

    pub fn new(adc: impl Peripheral<P = T> + 'd, _irq: impl interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + 'd, timer: impl Peripheral<P = Timer> + 'd, dma_ch: impl Peripheral<P = AdcDma> + 'd, _delay: &mut impl DelayUs<u32>) -> Self {
        into_ref!(adc, dma_ch, timer);
        T::enable_and_reset();
        Timer::enable_and_reset();

        // A.7.1 ADC calibration code example
        if T::regs().cr().read().aden() {
            T::regs().cr().modify(|w| w.set_addis(true));
        }
        while T::regs().cr().read().aden() {}
        T::regs().cfgr1().modify(|reg| reg.set_dmaen(false));
        T::regs().cr().modify(|reg| reg.set_adcal(true));
        while T::regs().cr().read().adcal() {}

        T::Interrupt::unpend();
        unsafe { T::Interrupt::enable() };

        Self {
            adc,
            timer,
            dma_ch,
        }
    }

    pub fn start(&mut self, sample_time: SampleTime, sample_freq: Hertz, pins: impl IntoIterator<Item=&'d mut dyn AdcPin<T>>, buf: &'d mut [u8]) -> ReadableRingBuffer<'d, AdcDma, u8>
    {
        const TRG4_TIM15_TRGO: u8 = 0b100;

        self.timer.stop();
        self.timer.set_frequency(sample_freq);
        self.timer.set_master_mode(stm32_metapac::timer::vals::Mms::UPDATE);

        // Clear the end of conversion, end of sampling flags, and overrun
        T::regs().isr().modify(|reg| {
            reg.set_eoc(true);
            reg.set_eosmp(true);
            reg.set_ovr(true);
        });

        // turn off interrupts
        T::regs().ier().modify(|w| {
            w.set_awdie(false);
            w.set_ovrie(false);
            w.set_eoseqie(false);
            w.set_eocie(false);
            w.set_eosmpie(false);
        });

        // enable selected channels
        T::regs().chselr().write(|w| w.0 = 0x0_u32);
        for pin in pins {
            pin.set_as_analog();
            let channel = pin.channel();
            T::regs().chselr().modify(|w| w.set_chselx(channel as usize, true));
        }

        // set the sampling time
        T::regs().smpr().modify(|reg| reg.set_smp(sample_time.into()));

        T::regs().cfgr1().modify(|reg| {
            reg.set_discen(false);
            reg.set_cont(false);
            reg.set_exten(stm32_metapac::adc::vals::Exten::BOTHEDGES);
            reg.set_extsel(TRG4_TIM15_TRGO);
            reg.set_scandir(stm32_metapac::adc::vals::Scandir::UPWARD);
            reg.set_dmacfg(stm32_metapac::adc::vals::Dmacfg::CIRCULAR);
            reg.set_dmaen(true);
            reg.set_align(stm32_metapac::adc::vals::Align::RIGHT);
            reg.set_res(stm32_metapac::adc::vals::Res::EIGHTBIT);
        });

        
        let request = self.dma_ch.request();
        let transfer_options = crate::dma::TransferOptions {
            circular: true,
            half_transfer_ir: true,
            complete_transfer_ir: true,
        };

        fn dr(r: crate::pac::adc::Adc) -> *mut u8 {
            r.dr().as_ptr() as _
        }

        let mut ring_buf = unsafe { ReadableRingBuffer::new(self.dma_ch.clone_unchecked(), request, dr(T::regs()), buf, transfer_options) };
        ring_buf.start();

        // Clear the ready bit
        T::regs().isr().modify(|w| w.set_adrdy(true));

        // Enable the ADC
        T::regs().cr().modify(|reg| reg.set_aden(true));

        // Wait for the ADC to become ready
        while !T::regs().isr().read().adrdy() {}

        // start the conversion (when the hardware trigger event occurs)
        T::regs().cr().modify(|w| w.set_adstart(true));

        self.timer.reset();
        self.timer.start();

        ring_buf
    }

    
}