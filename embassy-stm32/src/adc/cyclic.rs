
use core::marker::PhantomData;

use crate::adc::AdcPin;
use crate::adc::Instance;
use crate::adc::CyclicAdc;
use crate::dma::ringbuffer::DumbDmaRingBuf;
use crate::{interrupt, Peripheral};
use embassy_hal_internal::into_ref;
use crate::interrupt::typelevel::Interrupt;

/// Interrupt handler.
pub struct CyclicInterruptHandler<T: Instance> {
    _phantom: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for CyclicInterruptHandler<T> {
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


impl<'d, T, TimerInstance, DmaInstance>  CyclicAdc<'d, T, TimerInstance, DmaInstance>
where T: Instance,
    TimerInstance: crate::timer::BasicInstance,
    DmaInstance: crate::adc::AdcDma<T>,
{
    pub fn new(
            adc: impl Peripheral<P = T> + 'd,
            _irq: impl interrupt::typelevel::Binding<T::Interrupt, CyclicInterruptHandler<T>> + 'd,
            timer: impl Peripheral<P = TimerInstance> + 'd,
            dma: impl Peripheral<P = DmaInstance> + 'd
        ) -> Self {
        into_ref!(adc, dma);

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
            timer: crate::timer::low_level::Timer::new(timer),
            dma,
        }
    }

    pub fn start<'a>(&mut self, sample_time: super::SampleTime, sample_freq: crate::time::Hertz, pins: impl IntoIterator<Item=&'d mut dyn AdcPin<T>>, buffer: &'a mut [u8]) -> crate::dma::ringbuffer::DumbDmaRingBuf<'a, '_, u8> {

        self.timer.set_frequency(sample_freq);

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

        const TRG4_TIM15_TRGO: u8 = 0b100;
        T::regs().cfgr1().modify(|reg| {
            reg.set_discen(false);
            reg.set_cont(false);
            reg.set_exten(stm32_metapac::adc::vals::Exten::BOTHEDGES);
            reg.set_extsel(TRG4_TIM15_TRGO);
            reg.set_scandir(stm32_metapac::adc::vals::Scandir::UPWARD);
            reg.set_dmacfg(stm32_metapac::adc::vals::Dmacfg::CIRCULAR);
            reg.set_dmaen(true);
            reg.set_align(stm32_metapac::adc::vals::Align::RIGHT);
            reg.set_res(stm32_metapac::adc::vals::Res::BITS8);
        });

        let request = self.dma.request();
        let transfer_options = crate::dma::TransferOptions {
            priority: crate::dma::Priority::Medium,
            circular: true,
            half_transfer_ir: false,
            complete_transfer_ir: true,
        };

        let dma_channel = self.dma.reborrow().map_into();

        unsafe {
        dma_channel.configure(
            request,
            crate::dma::Dir::PeripheralToMemory,
            T::regs().dr().as_ptr() as *mut u32,
            buffer.as_mut_ptr() as *mut u32,
            buffer.len(),
            true,
            crate::dma::word::WordSize::OneByte,
            transfer_options,
        );
        }

        dma_channel.start();

        // Clear the ready bit
        T::regs().isr().modify(|w| w.set_adrdy(true));

        // Enable the ADC
        T::regs().cr().modify(|reg| reg.set_aden(true));

        // Wait for the ADC to become ready
        while !T::regs().isr().read().adrdy() {}

        // start the conversion (when the hardware trigger event occurs)
        T::regs().cr().modify(|w| w.set_adstart(true));

        self.timer.start();

        DumbDmaRingBuf {
            dma_buf: buffer,
            channel: dma_channel,
        }
        

    }
}