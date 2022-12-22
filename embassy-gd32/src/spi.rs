#![macro_use]

use crate::chip::peripherals;
use crate::{Hertz, Peripheral};
use crate::interrupt::{Interrupt, InterruptExt};
pub use embedded_hal_02::spi;
use embassy_hal_common::{into_ref, PeripheralRef};
use embedded_hal_02::spi::{Polarity, Phase};
pub use crate::chip::pac::spi0::ctl0::PSC_A;

pub struct Config {
    pub mode: spi::Mode,
    pub endian: Endian,
    pub clk_prescaler: Prescaler,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: spi::MODE_0,
            endian: Endian::MSB,
            clk_prescaler: Prescaler(PSC_A::DIV2),
        }
    }
}

pub enum Endian {
    MSB,
    LSB,
}

pub struct Prescaler(PSC_A);

impl From<PSC_A> for Prescaler {
    fn from(value: PSC_A) -> Self {
        Prescaler(value)
    }
}

impl crate::utils::ClockDivider for Prescaler {
    fn divide(&self, hz: Hertz) -> Hertz {
        match self.0 {
            PSC_A::DIV2 => Hertz(hz.0 / 2),
            PSC_A::DIV4 => Hertz(hz.0 / 4),
            PSC_A::DIV8 => Hertz(hz.0 / 8),
            PSC_A::DIV16 => Hertz(hz.0 / 16),
            PSC_A::DIV32 => Hertz(hz.0 / 32),
            PSC_A::DIV64 => Hertz(hz.0 / 64),
            PSC_A::DIV128 => Hertz(hz.0 / 128),
            PSC_A::DIV256 => Hertz(hz.0 / 256),
        }
    }
}

pub struct Spim<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
}

impl<'d, T> Spim<'d, T>
where
    T: Instance,
{
    pub fn new(
        spi: impl Peripheral<P = T> + 'd,
        sck: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        miso: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        mosi: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        config: Config,
    ) -> Self {

        into_ref!(spi, miso, mosi);

        // enable the clock to the SPI peripheral
        T::enable();

        let r = T::regs();
        r.ctl0.write(|w| {
            let w = match config.mode.polarity {
                Polarity::IdleLow => w.ckpl().clear_bit(),
                Polarity::IdleHigh => w.ckpl().set_bit(),
            };

            let w = match config.mode.phase {
                Phase::CaptureOnFirstTransition => w.ckph().clear_bit(),
                Phase::CaptureOnSecondTransition => w.ckph().set_bit(),
            };

            let w = match config.endian {
                Endian::MSB => w.lf().clear_bit(),
                Endian::LSB => w.lf().set_bit(),
            };

            let w = w.psc().variant(config.clk_prescaler.0);

            w
        });

        let gpio_speed = crate::gpio::Speed::from(T::frequency());

        let mut sck = crate::gpio::Flex::new(sck);
        sck.set_as_output(crate::gpio::OutputType::AFPushPull, gpio_speed);

        let mut miso = crate::gpio::Flex::new(miso);
        miso.set_as_input(crate::gpio::Pull::None);

        let mut mosi = crate::gpio::Flex::new(mosi);
        mosi.set_as_output(crate::gpio::OutputType::AFPushPull, gpio_speed);

        Self { _p: spi }
    }


}

pub(crate) mod sealed {
    use super::*;
    use embassy_sync::waitqueue::AtomicWaker;

    pub struct State {
        pub end_waker: AtomicWaker,
    }

    impl State {
        pub const fn new() -> Self {
            Self {
                end_waker: AtomicWaker::new(),
            }
        }
    }

    pub trait Instance {
        fn regs() -> &'static crate::pac::spi0::RegisterBlock;
        fn state() -> &'static State;
    }
}

pub trait Instance: Peripheral<P = Self> + sealed::Instance + crate::cctl::CCTLPeripherial +'static {
    type Interrupt: Interrupt;
}

macro_rules! impl_spi {
    ($type:ident, $pac_type:ident, $irq:ident) => {

        impl crate::spi::sealed::Instance for peripherals::$type {
            fn regs() -> &'static crate::pac::spi0::RegisterBlock {
                unsafe { &*crate::pac::$pac_type::ptr() }
            }

            fn state() -> &'static crate::spi::sealed::State {
                static STATE: crate::spi::sealed::State = crate::spi::sealed::State::new();
                &STATE
            }
        }

        impl crate::spi::Instance for peripherals::$type {
            type Interrupt = crate::interrupt::$irq;
        }

        impl crate::cctl::CCTLPeripherial for peripherals::$type {
            fn frequency() -> crate::utils::Hertz {
                let r = unsafe { &*crate::pac::$pac_type::ptr() };
                let prescaler = crate::spi::Prescaler::from(r.ctl0.read().psc().variant());
                let clocks = crate::cctl::get_freq();
                let pclk = match stringify!($type) {
                    "SPI0" => clocks.apb2,
                    "SPI1" => clocks.apb1,
                    "SPI2" => clocks.apb1,
                    _ => unreachable!(),
                };
                pclk / prescaler
            }
        
            fn enable() {
                let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
                match stringify!($type) {
                    "SPI0" => rcu.apb2en.modify(|_, w| w.spi0en().set_bit()),
                    "SPI1" => rcu.apb1en.modify(|_, w| w.spi1en().set_bit()),
                    "SPI2" => rcu.apb1en.modify(|_, w| w.spi2en().set_bit()),
                    _ => unreachable!(),
                }
            }
        
            fn disable() {
                let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
                match stringify!($type) {
                    "SPI0" => rcu.apb2en.modify(|_, w| w.spi0en().clear_bit()),
                    "SPI1" => rcu.apb1en.modify(|_, w| w.spi1en().clear_bit()),
                    "SPI2" => rcu.apb1en.modify(|_, w| w.spi2en().clear_bit()),
                    _ => unreachable!(),
                }
            }
        }
        
    };
}