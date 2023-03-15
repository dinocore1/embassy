#![macro_use]

use crate::pac::gpioa as gpio;
use crate::{into_ref, Hertz, Peripheral, PeripheralRef};

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Pull {
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Speed {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl From<Hertz> for Speed {
    fn from(value: Hertz) -> Self {
        if value <= Hertz::mhz(10) {
            Speed::Low
        } else if value <= Hertz::mhz(20) {
            Speed::Medium
        } else if value <= Hertz::mhz(50) {
            Speed::High
        } else {
            Speed::VeryHigh
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum OutputType {
    GPIOPushPull,
    GPIOOpenDrain,
    AFPushPull,
    AFOpenDrain,
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Level {
    Low,
    High,
}

impl From<bool> for Level {
    fn from(val: bool) -> Level {
        match val {
            true => Level::High,
            false => Level::Low,
        }
    }
}

impl Into<bool> for Level {
    fn into(self) -> bool {
        match self {
            Level::High => true,
            Level::Low => false,
        }
    }
}

pub struct Input<'d, T: Pin> {
    pub(crate) pin: Flex<'d, T>,
}

impl<'d, T: Pin> Input<'d, T> {
    pub fn new(pin: impl Peripheral<P = T> + 'd, pull: Pull) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_input(pull);
        Self { pin }
    }

    pub fn is_high(&self) -> bool {
        self.pin.is_high()
    }

    #[inline]
    pub fn is_low(&self) -> bool {
        self.pin.is_low()
    }

    #[inline]
    pub fn get_level(&self) -> Level {
        self.pin.get_level()
    }
}

pub struct Output<'d, T: Pin> {
    pub(crate) pin: Flex<'d, T>,
}

impl<'d, T: Pin> Output<'d, T> {
    #[inline]
    pub fn new(pin: impl Peripheral<P = T> + 'd, initial_output: Level, out_type: OutputType, speed: Speed) -> Self {
        let mut pin = Flex::new(pin);
        match initial_output {
            Level::High => pin.set_high(),
            Level::Low => pin.set_low(),
        }
        pin.set_as_output(out_type, speed);

        Self { pin }
    }

    #[inline]
    pub fn set_high(&mut self) {
        self.pin.set_high()
    }

    #[inline]
    pub fn set_low(&mut self) {
        self.pin.set_low()
    }
}

#[inline]
fn set_mode(mut reg: u32, mode: u32, pos: u8) -> u32 {
    reg &= !(0x0F << (4 *pos));
    reg |= mode << (4 * pos);
    reg
}

pub struct Flex<'d, T: Pin> {
    pub(crate) pin: PeripheralRef<'d, T>,
}

impl<'d, T: Pin> Flex<'d, T> {
    #[inline]
    pub fn new(pin: impl Peripheral<P = T> + 'd) -> Self {
        into_ref!(pin);
        Self { pin }
    }

    #[inline]
    pub fn set_as_input(&mut self, pull: Pull) {
        self.pin.set_as_input(pull);
    }

    #[inline]
    pub fn set_as_output(&mut self, out_type: OutputType, speed: Speed) {
        self.pin.set_as_output(out_type, speed);
    }

    #[inline]
    pub fn is_high(&self) -> bool {
        !self.is_low()
    }

    #[inline]
    pub fn is_low(&self) -> bool {
        self.pin.block().istat.read().bits() & (1 << self.pin.pin()) == 0
    }

    #[inline]
    pub fn get_level(&self) -> Level {
        self.is_high().into()
    }

    #[inline]
    pub fn set_high(&mut self) {
        self.pin.set_high()
    }

    #[inline]
    pub fn set_low(&mut self) {
        self.pin.set_low()
    }

    #[inline]
    pub fn set_level(&mut self, level: Level) {
        match level {
            Level::Low => self.pin.set_low(),
            Level::High => self.pin.set_high(),
        }
    }
}

pub(crate) mod sealed {

    use super::*;

    pub trait Pin {
        fn pin_port(&self) -> u8;

        #[inline]
        fn pin(&self) -> u8 {
            self.pin_port() % 16
        }

        #[inline]
        fn _port(&self) -> u8 {
            self.pin_port() / 16
        }

        #[inline]
        fn block(&self) -> &gpio::RegisterBlock {
            unsafe {
                match self._port() {
                    0 => &*crate::pac::GPIOA::ptr(),
                    1 => &*(crate::pac::GPIOB::ptr() as *const gpio::RegisterBlock),
                    2 => &*(crate::pac::GPIOC::ptr() as *const gpio::RegisterBlock),
                    _ => core::hint::unreachable_unchecked(),
                }
            }
        }

        #[inline]
        fn set_high(&self) {
            unsafe {
                let v = (1 << self.pin()) as u32;
                self.block().bop.write(|w| w.bits(v));
            }
        }

        #[inline]
        fn set_low(&self) {
            unsafe {
                let v = (1 << self.pin()) as u32;
                self.block().bc.write(|w| w.bits(v));
            }
        }

        #[inline]
        fn set_as_input(&mut self, pull: Pull) {
            critical_section::with(|_| {
                let r = self.block();
                let n = self.pin();

                let mode_value = match pull {
                    Pull::None => 0b0100_u32,
                    Pull::Up => {
                        r.octl.modify(|r, w| unsafe { w.bits(r.bits() | (1 << n)) });
                        0b1000_u32
                    }
                    Pull::Down => {
                        r.octl.modify(|r, w| unsafe { w.bits(r.bits() & !(1 << n)) });
                        0b1000_u32
                    }
                };

                if n <= 7 {
                    r.ctl0
                        .modify(|r, w| unsafe { w.bits(set_mode(r.bits(), mode_value, n)) });
                } else {
                    r.ctl1
                        .modify(|r, w| unsafe { w.bits(set_mode(r.bits(), mode_value, n - 8)) });
                }
            });
        }

        #[inline]
        fn set_as_output(&mut self, out_type: OutputType, speed: Speed) {
            critical_section::with(|_| {
                let r = self.block();
                let n = self.pin();

                let mode_value = match out_type {
                    OutputType::GPIOPushPull => 0b0000_u32,
                    OutputType::GPIOOpenDrain => 0b0100_u32,
                    OutputType::AFPushPull => 0b1000_u32,
                    OutputType::AFOpenDrain => 0b1100_u32,
                };

                let mode_value = match speed {
                    Speed::Low => mode_value | 0b01,
                    Speed::Medium => mode_value | 0b10,
                    Speed::High | Speed::VeryHigh => mode_value | 0b11,
                };

                if n <= 7 {
                    r.ctl0
                        .modify(|r, w| unsafe { w.bits(set_mode(r.bits(), mode_value, n)) });
                } else {
                    r.ctl1
                        .modify(|r, w| unsafe { w.bits(set_mode(r.bits(), mode_value, n - 8)) });
                }
            });
        }
    }
}

pub trait Pin: Peripheral<P = Self> + Into<AnyPin> + sealed::Pin + Sized + 'static {
    #[inline]
    fn degrade(self) -> AnyPin {
        AnyPin {
            pin_port: self.pin_port(),
        }
    }
}

pub struct AnyPin {
    pin_port: u8,
}

embassy_hal_common::impl_peripheral!(AnyPin);
impl Pin for AnyPin {}
impl sealed::Pin for AnyPin {
    #[inline]
    fn pin_port(&self) -> u8 {
        self.pin_port
    }
}

macro_rules! impl_pin {
    ($name:ident, $port_num:expr, $pin_num:expr) => {
        impl crate::gpio::Pin for peripherals::$name {}
        impl crate::gpio::sealed::Pin for peripherals::$name {
            #[inline]
            fn pin_port(&self) -> u8 {
                $port_num * 16 + $pin_num
            }
        }

        impl From<peripherals::$name> for AnyPin {
            fn from(x: peripherals::$name) -> Self {
                x.degrade()
            }
        }
    };
}

macro_rules! impl_gpio {
    ($name:ident, $port:expr) => {
        impl crate::gpio::GPIO for peripherals::$name {
            #[inline]
            fn port(&self) -> crate::gpio::GPIOPort {
                $port
            }
        }
    };
}

pub enum GPIOPort {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
}

pub trait GPIO: Peripheral<P = Self> + Sized + 'static {
    fn port(&self) -> GPIOPort;

    fn enable(&self) {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        match self.port() {
            GPIOPort::A => rcu.apb2en.modify(|_, w| w.paen().set_bit()),
            GPIOPort::B => rcu.apb2en.modify(|_, w| w.pben().set_bit()),
            GPIOPort::C => rcu.apb2en.modify(|_, w| w.pcen().set_bit()),
            GPIOPort::D => rcu.apb2en.modify(|_, w| w.pden().set_bit()),
            GPIOPort::E => rcu.apb2en.modify(|_, w| w.peen().set_bit()),
            GPIOPort::F => rcu.apb2en.modify(|_, w| w.pfen().set_bit()),
            GPIOPort::G => rcu.apb2en.modify(|_, w| w.pgen().set_bit()),
        }
    }

    fn disable(&self) {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        match self.port() {
            GPIOPort::A => rcu.apb2en.modify(|_, w| w.paen().clear_bit()),
            GPIOPort::B => rcu.apb2en.modify(|_, w| w.pben().clear_bit()),
            GPIOPort::C => rcu.apb2en.modify(|_, w| w.pcen().clear_bit()),
            GPIOPort::D => rcu.apb2en.modify(|_, w| w.pden().clear_bit()),
            GPIOPort::E => rcu.apb2en.modify(|_, w| w.peen().clear_bit()),
            GPIOPort::F => rcu.apb2en.modify(|_, w| w.pfen().clear_bit()),
            GPIOPort::G => rcu.apb2en.modify(|_, w| w.pgen().clear_bit()),
        }
    }
}
