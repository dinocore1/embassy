use super::*;
use crate::dma::ReadableRingBuffer;
use crate::time::Hertz;
use crate::{interrupt, Peripheral};
use embassy_hal_internal::{into_ref, PeripheralRef};
use embedded_hal_02::blocking::delay::DelayUs;
use crate::interrupt::typelevel::Interrupt;
use crate::timer::sealed::Basic16bitInstance as BasicTimer;


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

    pub fn new(adc: impl Peripheral<P = T> + 'd, timer: impl Peripheral<P = Timer> + 'd, dma_ch: impl Peripheral<P = AdcDma> + 'd, delay: &mut impl DelayUs<u32>) -> Self {
        into_ref!(adc, dma_ch, timer);
        T::enable_and_reset();
        Timer::enable_and_reset();

        // Delay 1μs when using HSI14 as the ADC clock.
        //
        // Table 57. ADC characteristics
        // tstab = 14 * 1/fadc
        delay.delay_us(1);

        // A.7.1 ADC calibration code example
        T::regs().cfgr1().modify(|reg| reg.set_dmaen(false));
        T::regs().cr().modify(|reg| reg.set_adcal(true));
        while T::regs().cr().read().adcal() {}

        // // A.7.2 ADC enable sequence code example
        // if T::regs().isr().read().adrdy() {
        //     T::regs().isr().modify(|reg| reg.set_adrdy(true));
        // }
        // T::regs().cr().modify(|reg| reg.set_aden(true));
        // while !T::regs().isr().read().adrdy() {
        //     // ES0233, 2.4.3 ADEN bit cannot be set immediately after the ADC calibration
        //     // Workaround: When the ADC calibration is complete (ADCAL = 0), keep setting the
        //     // ADEN bit until the ADRDY flag goes high.
        //     T::regs().cr().modify(|reg| reg.set_aden(true));
        // }

        T::Interrupt::unpend();

        Self {
            adc,
            timer,
            dma_ch,
        }
    }

    pub fn start(&mut self, sample_time: SampleTime, sample_freq: Hertz, channels: u32, buf: &'d mut [u8]) {
        

        const TRG4_TIM15_TRGO: u8 = 0b100;

        self.timer.stop();
        self.timer.set_frequency(sample_freq);

        // Clear the end of conversion and end of sampling flags
        T::regs().isr().modify(|reg| {
            reg.set_eoc(true);
            reg.set_eosmp(true);
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
        T::regs().chselr().write(|w| w.0 = channels);

        // set the sampling time
        T::regs().smpr().modify(|reg| reg.set_smp(sample_time.into()));

        T::regs().cfgr1().modify(|reg| {
            reg.set_discen(false);
            reg.set_cont(false);
            reg.set_exten(stm32_metapac::adc::vals::Exten::FALLINGEDGE);
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
            complete_transfer_ir: false,
        };

        fn dr(r: crate::pac::adc::Adc) -> *mut u8 {
            r.dr().as_ptr() as _
        }

        let mut ring_buf = unsafe { ReadableRingBuffer::new(self.dma_ch.clone_unchecked(), request, dr(T::regs()), buf, transfer_options) };
        ring_buf.start();

        T::regs().cr().modify(|reg| reg.set_aden(true));

        self.timer.reset();
        self.timer.start();


    }

    
}