#![macro_use]

use crate::{Hertz, Peripheral};
use crate::interrupt::{Interrupt, InterruptExt};
pub use embedded_hal_02::spi;
use embassy_hal_common::{into_ref, PeripheralRef};

pub struct Config {
    pub freq: Hertz,
    pub mode: spi::Mode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            freq: Hertz(1_000_000),
            mode: spi::MODE_0,
        }
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

pub trait Instance: Peripheral<P = Self> + sealed::Instance + 'static {
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
        
    };
}