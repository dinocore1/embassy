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

fn read<'a, C: Channel, S, D>(
    ch: impl Peripheral<P = C> + 'a,
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
        .mwidth().variant(D::width().into_p())
        .pwidth().variant(S::width().into_p())
        .mnaga().set_bit()
        .ftfie().set_bit()
        .chen().set_bit()
    );

    

    unsafe {
    move_data(C::Instance::regs(), C::number(), dest as *const (), src as *const (), ctrl_reg_val, count);
    }

    into_ref!(ch);
    Transfer::new(ch)
    

}

unsafe fn move_data(
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

    cnt_reg.write_volatile(count as u32);
    membase_reg.write_volatile(mem_addr as u32);
    perbase_reg.write_volatile(per_addr as u32);

    ctl_reg.write_volatile(ctrl_reg_val);

    //crate::pac::dma0::CH0CTL

    //ctl_reg.write(|w| w.bits(ctrl_reg));
    

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

