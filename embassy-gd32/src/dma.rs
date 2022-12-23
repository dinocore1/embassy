#![macro_use]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use embassy_hal_common::{into_ref, PeripheralRef};

use crate::chip::peripherals;
use crate::{Hertz, Peripheral};


pub struct Transfer<'a, C: Channel> {
    channel: PeripheralRef<'a, C>,
}

impl<'a, C: Channel> Transfer<'a, C> {
    pub(crate) fn new(channel: impl Peripheral<P = C> + 'a) -> Self {
        into_ref!(channel);
        Self { channel }
    }
}

impl<'a, C: Channel> Unpin for Transfer<'a, C> {}
impl<'a, C: Channel> Future for Transfer<'a, C> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let channel = C::number();
        C::Instance::wakers()[channel as usize].register(cx.waker());

        let r = C::Instance::regs();
        

        todo!()
    }
}

pub trait Instance: Peripheral<P = Self> + 'static {

    fn wakers() -> &'static [embassy_sync::waitqueue::AtomicWaker];

    fn regs() -> &'static crate::pac::dma0::RegisterBlock;
}

pub trait Channel: Peripheral<P = Self> + 'static {
    type Instance: Instance;

    fn number() -> u8;
}

macro_rules! impl_dma {
    ($type:ident, $pac_type:ident, $num_channels:expr) => {

        impl crate::dma::Instance for peripherals::$type {

            fn wakers() -> &'static [embassy_sync::waitqueue::AtomicWaker] {
                use embassy_sync::waitqueue::AtomicWaker;
                const NEW_AW: AtomicWaker = AtomicWaker::new();
                static wakers: [AtomicWaker ; $num_channels] = [NEW_AW ; $num_channels];
                &wakers
            }

            fn regs() -> &'static crate::pac::dma0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::dma0::RegisterBlock) }
            }
        }
        
    };
}

macro_rules! impl_dma_channel {
    ($type:ident, $inst:ident, $ch:expr) => {

        impl crate::dma::Channel for peripherals::$type {
            type Instance = peripherals::$inst;

            fn number() -> u8 {
                $ch
            }
        }

    };
}

