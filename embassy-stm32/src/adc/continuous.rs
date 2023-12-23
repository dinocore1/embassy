use super::*;
use crate::time::Hertz;
use crate::{interrupt, Peripheral};
use embassy_hal_internal::{into_ref, PeripheralRef};
use embedded_hal_02::blocking::delay::DelayUs;
use crate::interrupt::typelevel::Interrupt;
use crate::timer::sealed::Basic16bitInstance as BasicTimer;


pub struct ContinuousAdc<'d, T, TIM>
{
    #[allow(unused)]
    adc: PeripheralRef<'d, T>,
    timer: PeripheralRef<'d, TIM>,
}

impl<'d, T: Instance, TIM> ContinuousAdc<'d, T, TIM>
where T: Instance,
    TIM: BasicTimer,
{

    pub fn new(adc: impl Peripheral<P = T> + 'd, timer: impl Peripheral<P = TIM> + 'd, delay: &mut impl DelayUs<u32>) -> Self {
        into_ref!(adc, timer);
        T::enable_and_reset();
        TIM::enable_and_reset();

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
        }
    }

    pub fn start(&mut self, sample_time: SampleTime, sample_freq: Hertz, channels: u32, dma_ch: impl Peripheral<P = impl super::AdcDma<T>>) {
        into_ref!(dma_ch);

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
            reg.set_scandir(stm32_metapac::adc::vals::Scandir::UPWARD);
            reg.set_dmacfg(stm32_metapac::adc::vals::Dmacfg::CIRCULAR);
            reg.set_dmaen(true);
        });

        let mut buf = [0_u8 ; 32];
        let request = dma_ch.request();
        let transfer_options = crate::dma::TransferOptions {
            circular: true,
            half_transfer_ir: true,
            complete_transfer_ir: false,
        };
        //let transfer = crate::dma::Transfer::new_read(dma_ch, (), T::regs().dr().as_ptr(), &mut buf, transfer_options);


    }

    
}