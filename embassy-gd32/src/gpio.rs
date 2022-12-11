#![macro_use]

use crate::{Peripheral, into_ref, PeripheralRef};
use crate::pac::gpioa as gpio;

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Pull {
    None,
    Up,
    Down,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Speed {
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Debug, Eq, PartialEq)]
pub enum OutputType {
    PushPull,
    OpenDrain,
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
        critical_section::with(|_|  {
            let r = self.pin.block();
            let n = self.pin.pin();

            let v = match pull {
                Pull::None => 0b0100_u32,
                Pull::Up => {
                    r.octl.modify(|_, w| unsafe { w.bits(1 << n) });
                    0b1000_u32
                },
                Pull::Down => {
                    r.octl.modify(|r, w| {
                        let v = r.bits() & !(1 << n);
                        unsafe { w.bits(v) }
                    });
                    0b1000_u32
                },
            };
            
            if n <= 7 {
                r.ctl0.modify(|_, w| unsafe { w.bits(v << (4*n)) });
            } else {
                r.ctl1.modify(|_, w| unsafe { w.bits(v << (4*(n-8))) });
            }
        });
        
    }

    #[inline]
    pub fn set_as_output(&mut self, out_type: OutputType, speed: Speed) {
        critical_section::with(|_|  {
            let r = self.pin.block();
            let n = self.pin.pin();

            let v = match out_type {
                OutputType::PushPull => 0b0000_u32,
                OutputType::OpenDrain => 0b0100_u32,
            };

            let v = match speed {
                Speed::Low => v | 0b01,
                Speed::Medium => v | 0b10,
                Speed::High | Speed::VeryHigh => v | 0b11,
            };

            if n <= 7 {
                r.ctl0.modify(|_, w| unsafe { w.bits(v << (4*n)) });
            } else {
                r.ctl1.modify(|_, w| unsafe { w.bits(v << (4*(n-8))) });
            }

        });

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

    }

}

pub trait Pin: Peripheral<P = Self> + sealed::Pin + Sized + 'static {

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
    }
}

pub enum GPIOPort {
    A,
    B,
    C,
    D,
    E,
    F,
    G
}

pub trait GPIO: Peripheral<P = Self> + Sized + 'static {
    fn port(&self) -> GPIOPort;

    fn enable(&self) {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        match self.port() {
            GPIOPort::A => rcu.apb2en.modify(|_,w| w.paen().set_bit()),
            GPIOPort::B => rcu.apb2en.modify(|_,w| w.pben().set_bit()),
            GPIOPort::C => rcu.apb2en.modify(|_,w| w.pcen().set_bit()),
            GPIOPort::D => rcu.apb2en.modify(|_,w| w.pden().set_bit()),
            GPIOPort::E => rcu.apb2en.modify(|_,w| w.peen().set_bit()),
            GPIOPort::F => rcu.apb2en.modify(|_,w| w.pfen().set_bit()),
            GPIOPort::G => rcu.apb2en.modify(|_,w| w.pgen().set_bit()),
        }
    }

    fn disable(&self) {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        match self.port() {
            GPIOPort::A => rcu.apb2en.modify(|_,w| w.paen().clear_bit()),
            GPIOPort::B => rcu.apb2en.modify(|_,w| w.pben().clear_bit()),
            GPIOPort::C => rcu.apb2en.modify(|_,w| w.pcen().clear_bit()),
            GPIOPort::D => rcu.apb2en.modify(|_,w| w.pden().clear_bit()),
            GPIOPort::E => rcu.apb2en.modify(|_,w| w.peen().clear_bit()),
            GPIOPort::F => rcu.apb2en.modify(|_,w| w.pfen().clear_bit()),
            GPIOPort::G => rcu.apb2en.modify(|_,w| w.pgen().clear_bit()),
        }
    }


}