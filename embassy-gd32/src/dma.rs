#![macro_use]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use embassy_hal_common::{into_ref, PeripheralRef};

use crate::chip::peripherals;
use crate::{interrupt, Hertz, Peripheral};

pub(crate) mod waker {
    use core::cell::UnsafeCell;
    use core::task::Waker;

    pub struct DmaWaker {
        waker: UnsafeCell<Option<Waker>>,
    }

    unsafe impl Send for DmaWaker {}
    unsafe impl Sync for DmaWaker {}
    
    impl DmaWaker {
        pub(crate) const fn new() -> Self {
            Self {
                waker: UnsafeCell::new(None),
            }
        }

        pub fn register<'a>(&self, w: &Waker, cs: critical_section::CriticalSection<'a>) {
            let waker = unsafe { &mut *self.waker.get() };
            
            match waker {
                None => {
                    *waker = Some(w.clone());
                }

                Some(w2) => {
                    if !w2.will_wake(w) {
                        panic!("cant handle two tasks waiting on the same thing");
                    }
                }
            }

        }

        pub fn wake(&self) {
            let waker = critical_section::with(|cs| {
                let waker = unsafe { &mut *self.waker.get() };
                waker.take()
            });
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    TransferError
}

pub struct Transfer<'a, C: Channel> {
    channel: PeripheralRef<'a, C>,
}

impl<'a, C: Channel> Transfer<'a, C> {
    pub(crate) fn new(channel: impl Peripheral<P = C> + 'a) -> Self {
        into_ref!(channel);
        Self { channel }
    }
}

pub fn read<'a, C: Channel, S, D>(
    ch: PeripheralRef<'a, C>,
    src: *const S,
    dest: *mut D,
    count: u16
) -> Transfer<'a, C>
where
 C: Channel,
 S: Word,
 D: Word,
{
    let mut ctrl_reg_val = 0;
    let ctrl_reg = unsafe { &*((&mut ctrl_reg_val) as *mut _ as *mut crate::pac::dma0::CH0CTL) };

    ctrl_reg.write(|w| w
        .mwidth().variant(D::width().into_p())
        .pwidth().variant(S::width().into_p())
        .mnaga().set_bit()
        .dir().variant(crate::pac::dma0::ch0ctl::DIR_A::FROM_PERIPHERAL)
        .ftfie().set_bit()
        .chen().set_bit()
    );

    unsafe {
        configure_channel(C::Instance::regs(), C::number(), dest as *const (), src as *const (), ctrl_reg_val, count);
    }

    into_ref!(ch);
    Transfer::new(ch)
}

pub fn write<'a, C: Channel, S, D>(
    ch: PeripheralRef<'a, C>,
    src: *const S,
    dest: *mut D,
    count: u16
) -> Transfer<'a, C>
where
 C: Channel,
 S: Word,
 D: Word,
{
    let mut ctrl_reg_val = 0;

    let ctrl_reg = unsafe { &*(&mut ctrl_reg_val as *mut _ as *mut crate::pac::dma0::CH0CTL) };

    ctrl_reg.write(|w| w
        .mwidth().variant(S::width().into_p())
        .pwidth().variant(D::width().into_p())
        .mnaga().set_bit()
        .dir().variant(crate::pac::dma0::ch0ctl::DIR_A::FROM_MEMORY)
        .ftfie().set_bit()
        .chen().set_bit()
    );

    unsafe {
        configure_channel(C::Instance::regs(), C::number(), dest as *const (), src as *const (), ctrl_reg_val, count);
    }

    into_ref!(ch);
    Transfer::new(ch)
}

unsafe fn configure_channel(
    instance_regs: &'static crate::pac::dma0::RegisterBlock,
    ch_num: u8,
    mem_addr: *const (),
    per_addr: *const (),
    ctrl_reg_val: u32,
    count: u16,

) {
    let reg_base = instance_regs as *const _ as *mut u8;
    let membase_reg = reg_base.offset((0x14 * ch_num as isize) + 0x14).cast::<u32>();
    let perbase_reg = reg_base.offset((0x14 * ch_num as isize) + 0x10).cast::<u32>();
    let cnt_reg = reg_base.offset((0x14 * ch_num as isize) + 0xC).cast::<u32>();
    let ctl_reg = reg_base.offset((0x14 * ch_num as isize) + 0x8).cast::<u32>();

    // disable the channel
    ctl_reg.write_volatile(0);

    cnt_reg.write_volatile(count as u32);
    membase_reg.write_volatile(mem_addr as u32);
    perbase_reg.write_volatile(per_addr as u32);

    ctl_reg.write_volatile(ctrl_reg_val);
}

impl<'a, C: Channel> Unpin for Transfer<'a, C> {}
impl<'a, C: Channel> Future for Transfer<'a, C> {
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let channel = C::number();
        let inst = C::Instance::regs();
        let gifx = 1_u32 << (4 * channel);

        critical_section::with(|cs| {
            let intf = inst.intf.read();
            if intf.bits() & gifx != 0 {
                //clear all channel interrupt flags
                let all_if = 0x0F_u32 << (4 * channel);
                inst.intc.write(|w| unsafe { w.bits(all_if) } );
    
                let errif = 1_u32 << (4 * channel + 3);
                if intf.bits() & errif != 0 {
                    Poll::Ready(Err(Error::TransferError))
                } else {
                    Poll::Ready(Ok(()))
                }
                
            } else {
                C::Instance::wakers()[channel as usize].register(cx.waker(), cs);
                Poll::Pending
            }
        })
        
    }
}

#[repr(u8)]
pub enum Width {
    Bits8 = 0b00,
    Bits16 = 0b01,
    Bits32 = 0b10,
}

impl Width {
    #[inline(always)]
    fn into_p(&self) -> crate::pac::dma0::ch0ctl::PWIDTH_A {
        match self {
            Width::Bits8 => crate::pac::dma0::ch0ctl::PWIDTH_A::BITS8,
            Width::Bits16 => crate::pac::dma0::ch0ctl::PWIDTH_A::BITS16,
            Width::Bits32 => crate::pac::dma0::ch0ctl::PWIDTH_A::BITS32,
        }
    }

    
}

impl From<Width> for u8 {
    #[inline(always)]
    fn from(v: Width) -> Self {
        v as _
    }
}

mod sealed {
    pub trait Word {}
}

pub trait Word: sealed::Word {
    fn width() -> Width;
}

impl sealed::Word for u8 {}
impl Word for u8 {
    fn width() -> Width {
        Width::Bits8
    }
}

impl sealed::Word for u16 {}
impl Word for u16 {
    fn width() -> Width {
        Width::Bits16
    }
}

impl sealed::Word for u32 {}
impl Word for u32 {
    fn width() -> Width {
        Width::Bits32
    }
}

pub trait Instance: Peripheral<P = Self> + 'static {

    fn wakers() -> &'static [waker::DmaWaker];

    fn regs() -> &'static crate::pac::dma0::RegisterBlock;
}

pub trait Channel: Peripheral<P = Self> + 'static {
    type Instance: Instance;

    fn number() -> u8;
}

macro_rules! impl_dma {
    ($type:ident, $pac_type:ident, $num_channels:expr) => {

        impl crate::dma::Instance for peripherals::$type {

            fn wakers() -> &'static [crate::dma::waker::DmaWaker] {
                const NEW_AW: crate::dma::waker::DmaWaker = crate::dma::waker::DmaWaker::new();
                static WAKERS: [crate::dma::waker::DmaWaker ; $num_channels] = [NEW_AW ; $num_channels];
                &WAKERS
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



#[interrupt]
unsafe fn DMA0_CHANNEL0() {
    debug!("DMA0_CHANNEL0");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif0().bit_is_set() {
        error!("DMA0_CHANNEL0: error");
    }
    crate::chip::peripherals::DMA0::wakers()[0].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL1() {
    debug!("DMA0_CHANNEL1");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif1().bit_is_set() {
        error!("DMA0_CHANNEL1: error");
    }
    crate::chip::peripherals::DMA0::wakers()[1].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL2() {
    debug!("DMA0_CHANNEL2");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif2().bit_is_set() {
        error!("DMA0_CHANNEL2: error");
    }
    crate::chip::peripherals::DMA0::wakers()[2].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL3() {
    debug!("DMA0_CHANNEL3");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif3().bit_is_set() {
        error!("DMA0_CHANNEL3: error");
    }
    crate::chip::peripherals::DMA0::wakers()[3].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL4() {
    debug!("DMA0_CHANNEL4");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif4().bit_is_set() {
        error!("DMA0_CHANNEL4: error");
    }
    crate::chip::peripherals::DMA0::wakers()[4].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL5() {
    debug!("DMA0_CHANNEL5");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif5().bit_is_set() {
        error!("DMA0_CHANNEL5: error");
    }
    crate::chip::peripherals::DMA0::wakers()[5].wake();
}

#[interrupt]
unsafe fn DMA0_CHANNEL6() {
    debug!("DMA0_CHANNEL6");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif6().bit_is_set() {
        error!("DMA0_CHANNEL6: error");
    }
    crate::chip::peripherals::DMA0::wakers()[6].wake();
}

