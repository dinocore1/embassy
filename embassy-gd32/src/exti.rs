
use core::marker::PhantomData;
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;
use futures::Future;

use crate::pac::EXTI;
use crate::gpio::{AnyPin, Input, Pin as GpioPin};
use crate::{peripherals, Peripheral};

pub use crate::pac::AFIO;

const EXTI_COUNT: usize = 16;
const NEW_AW: AtomicWaker = AtomicWaker::new();
static EXTI_WAKERS: [AtomicWaker; EXTI_COUNT] = [NEW_AW; EXTI_COUNT];

pub fn steal_AFIO() -> AFIO {
    unsafe { crate::pac::Peripherals::steal().AFIO }
}

pub fn steal_EXTI() -> EXTI {
    unsafe { crate::pac::Peripherals::steal().EXTI }
}

/// EXTI input driver
pub struct ExtiInput<'d, T: GpioPin> {
    pin: Input<'d, T>,
}

impl<'d, T: GpioPin> Unpin for ExtiInput<'d, T> {}

impl<'d, T: GpioPin> ExtiInput<'d, T> {
    pub fn new(pin: Input<'d, T>, _ch: impl Peripheral<P = T::ExtiChannel> + 'd) -> Self {
        Self { pin }
    }

    pub fn is_high(&self) -> bool {
        self.pin.is_high()
    }

    pub fn is_low(&self) -> bool {
        self.pin.is_low()
    }

    pub async fn wait_for_rising_edge(&mut self) {
        let fut = ExtiInputFuture::new(self.pin.pin.pin.pin(), self.pin.pin.pin.port(), true, false);
        fut.await
    }
}

struct ExtiInputFuture<'a> {
    pin: u8,
    phantom: PhantomData<&'a mut AnyPin>,
}

impl<'a> ExtiInputFuture<'a> {

    fn new(pin: u8, port: u8, rising: bool, falling: bool) -> Self {

        
        critical_section::with(|_| {

            //TODO: set the GPIO exti source select

            let exti = unsafe { crate::pac::Peripherals::steal().EXTI };
            let v = 1_u32 << pin;

            #[inline]
            fn clear_bit(mut v: u32, mask: u32) -> u32 {
                v &= !mask;
                v
            }

            exti.inten.modify(|r, w| unsafe { w.bits(r.bits() | v) } );

            if rising {
                exti.rten.modify(|r, w| unsafe { w.bits(r.bits() | v)});
            } else {
                exti.rten.modify(|r, w| unsafe { w.bits(clear_bit(r.bits(), v))});
            }

            if falling {
                exti.ften.modify(|r, w| unsafe { w.bits(r.bits() | v)});
            } else {
                exti.ften.modify(|r, w| unsafe { w.bits(clear_bit(r.bits(), v))});
            }

            // clear the pending flag
            exti.pd.write(|w| unsafe { w.bits(v) });

        });
        

        Self {
            pin,
            phantom: PhantomData,
        }
    }

}

impl<'a> Future for ExtiInputFuture<'a> {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
        
        let exti = unsafe { crate::pac::Peripherals::steal().EXTI };

        EXTI_WAKERS[self.pin as usize].register(cx.waker());
        let v = 1_u32 << self.pin;
        let inten = exti.inten.read().bits();
        if (inten & v) == 0 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

pub(crate) mod sealed {
    pub trait Channel {}
}

pub trait Channel: sealed::Channel + Sized {
    fn number(&self) -> usize;
    fn degrade(self) -> AnyChannel {
        AnyChannel {
            number: self.number() as u8,
        }
    }
}

pub struct AnyChannel {
    number: u8,
}
embassy_hal_common::impl_peripheral!(AnyChannel);
impl sealed::Channel for AnyChannel {}
impl Channel for AnyChannel {
    fn number(&self) -> usize {
        self.number as usize
    }
}

macro_rules! impl_exti {
    ($type:ident, $number:expr) => {
        impl sealed::Channel for peripherals::$type {}
        impl Channel for peripherals::$type {
            fn number(&self) -> usize {
                $number as usize
            }
        }
    };
}

impl_exti!(EXTI0, 0);
impl_exti!(EXTI1, 1);
impl_exti!(EXTI2, 2);
impl_exti!(EXTI3, 3);
impl_exti!(EXTI4, 4);
impl_exti!(EXTI5, 5);
impl_exti!(EXTI6, 6);
impl_exti!(EXTI7, 7);
impl_exti!(EXTI8, 8);
impl_exti!(EXTI9, 9);
impl_exti!(EXTI10, 10);
impl_exti!(EXTI11, 11);
impl_exti!(EXTI12, 12);
impl_exti!(EXTI13, 13);
impl_exti!(EXTI14, 14);
impl_exti!(EXTI15, 15);