#![macro_use]

use crate::chip::peripherals;
use crate::{Hertz, Peripheral};
use crate::interrupt::{Interrupt, InterruptExt};
pub use embedded_hal_02::spi as hal;
use embassy_hal_common::{into_ref, PeripheralRef};
use embedded_hal_02::spi::{Polarity, Phase};
use crate::pac::spi0::RegisterBlock as Regs;

pub struct Config {
    pub mode: hal::Mode,
    pub endian: Endian,
    pub clk_prescaler: Prescaler,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: hal::MODE_0,
            endian: Endian::MSB,
            clk_prescaler: Prescaler::DIV2,
        }
    }
}

pub enum Endian {
    MSB,
    LSB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Prescaler {
    DIV2 = 0,
    DIV4 = 1,
    DIV8 = 2,
    DIV16  = 3,
    DIV32 = 4,
    DIV64 = 5,
    DIV128 = 6,
    DIV256 = 7,
}

impl From<Prescaler> for u8 {
    #[inline(always)]
    fn from(variant: Prescaler) -> Self {
        variant as _
    }
}

impl Prescaler {
    #[inline(always)]
    fn from_bits(bits: u8) -> Self {
        unsafe { core::mem::transmute(bits) }
        // match bits {
        //     0 => Prescaler::DIV2,
        //     1 => Prescaler::DIV4,
        //     2 => Prescaler::DIV8,
        //     3 => Prescaler::DIV16,
        //     4 => Prescaler::DIV32,
        //     5 => Prescaler::DIV64,
        //     6 => Prescaler::DIV128,
        //     7 => Prescaler::DIV256,
        //     _ => unreachable!(),
        // }
    }
}

impl crate::utils::ClockDivider for Prescaler {
    fn divide(&self, hz: Hertz) -> Hertz {
        match self {
            Prescaler::DIV2 => Hertz(hz.0 / 2),
            Prescaler::DIV4 => Hertz(hz.0 / 4),
            Prescaler::DIV8 => Hertz(hz.0 / 8),
            Prescaler::DIV16 => Hertz(hz.0 / 16),
            Prescaler::DIV32 => Hertz(hz.0 / 32),
            Prescaler::DIV64 => Hertz(hz.0 / 64),
            Prescaler::DIV128 => Hertz(hz.0 / 128),
            Prescaler::DIV256 => Hertz(hz.0 / 256),
        }
    }
}

pub struct Spim<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
}

impl<'d, T: Instance> Spim<'d, T>
{
    pub fn new(
        spi: impl Peripheral<P = T> + 'd,
        irq: impl Peripheral<P = T::Interrupt> + 'd,
        sck: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        miso: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        mosi: impl Peripheral<P = impl crate::gpio::Pin> + 'd,
        config: Config,
    ) -> Self {

        into_ref!(spi, miso, mosi, irq);

        irq.set_handler(Self::on_interrupt);
        irq.unpend();
        irq.enable();

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

            let w = w.psc().bits(u8::from(config.clk_prescaler));

            // config for master mode full-duplex
            let w = w.mstmod().set_bit();
            let w = w.ro().clear_bit();
            let w = w.bden().clear_bit();

            let w = w.spien().set_bit();

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

    fn on_interrupt(_: *mut()) {
        let r = T::regs();
        let s = T::state();

        
    }

    fn prepare(&mut self, tx: &[u8], rx: &mut [u8]) {
        
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

pub trait Instance: Peripheral<P = Self> + sealed::Instance + crate::cctl::CCTLPeripherial + 'static {
    type Interrupt: Interrupt;
}

macro_rules! impl_spi {
    ($type:ident, $pac_type:ident, $irq:ident) => {

        impl crate::spi::sealed::Instance for peripherals::$type {
            fn regs() -> &'static crate::pac::spi0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::spi0::RegisterBlock) }
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


impl crate::cctl::CCTLPeripherial for peripherals::SPI0 {
    fn frequency() -> crate::utils::Hertz {
        let r = unsafe { &*crate::pac::SPI0::ptr() };
        let prescaler = crate::spi::Prescaler::from_bits(r.ctl0.read().psc().bits());
        let clocks = crate::cctl::get_freq();
        clocks.apb2 / prescaler
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.spi0en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.spi0en().clear_bit())
    }
}

impl crate::cctl::CCTLPeripherial for peripherals::SPI1 {
    fn frequency() -> crate::utils::Hertz {
        let r = unsafe { &*crate::pac::SPI1::ptr() };
        let prescaler = crate::spi::Prescaler::from_bits(r.ctl0.read().psc().bits());
        let clocks = crate::cctl::get_freq();
        clocks.apb1 / prescaler
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.spi1en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.spi1en().clear_bit())
    }
}


