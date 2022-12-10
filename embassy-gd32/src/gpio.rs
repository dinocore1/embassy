

use crate::{Peripheral, into_ref, PeripheralRef};
use crate::pac::gpioa as gpio;

use self::sealed::Pin;


#[derive(Debug, Eq, PartialEq)]
pub enum Pull {
    None,
    Up,
    Down,
}

pub struct Input<'d, T: Pin> {
    pub(crate) pin: Flex<'d, T>,
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
        let r = self.pin.block();
        let n = self.pin.pin();

        let (v, octl) = match pull {
            Pull::None => (0b0100_u32, 0_u8),
            Pull::Up => (0b1000_u32, 1_u8),
            Pull::Down => (0b1000_u32, 0_u8),
        };
        
        if n <= 7 {
            r.ctl0.modify(|_, w| unsafe { w.bits(v << (4*n)) });
        } else {
            r.ctl1.modify(|_, w| unsafe { w.bits(v << (4*(n-8))) });
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