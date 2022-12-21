#![macro_use]

use crate::chip::peripherals;
use crate::{Hertz, Peripheral};
use crate::interrupt::{Interrupt, InterruptExt};
pub use embedded_hal_02::spi;
use embassy_hal_common::{into_ref, PeripheralRef};
use embedded_hal_02::spi::{Polarity, Phase};

pub struct Config {
    pub freq: Hertz,
    pub mode: spi::Mode,
    pub endian: Endian,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            freq: Hertz(1_000_000),
            mode: spi::MODE_0,
            endian: Endian::MSB,
        }
    }
}

pub enum Endian {
    MSB,
    LSB,
}

pub struct Prescaler(u8);

impl crate::utils::ClockDivider for Prescaler {
    fn divide(&self, hz: Hertz) -> Hertz {
        let div = 1 << (self.0 + 2);
        Hertz::hz(hz.0 / div)
    }
}

pub struct Spim<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
}

impl<'d, T: Instance> Spim<'d, T> {
    pub fn new(
        spi: impl Peripheral<P = T> + 'd,
        sck: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        miso: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        mosi: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        config: Config,
    ) -> Self {
        into_ref!(sck, miso, mosi);

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

            w

            
        });


        into_ref!(spi);
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
                let prescaler = Prescaler(r.ctl0.read().psc().bits());
                crate::cctl::get_freq().sys / prescaler
            }
        
            fn enable() {
                todo!()
            }
        
            fn disable() {
                todo!()
            }
        }
        
    };
}