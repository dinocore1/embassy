#![macro_use]

use core::cell::UnsafeCell;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

use embassy_cortex_m::interrupt::Interrupt;
use embassy_hal_common::{into_ref, PeripheralRef};
use embassy_sync::waitqueue::WakerRegistration;
use crate::cctl::CCTLPeripherial;

use crate::{interrupt, Peripheral};

struct ChannelStateInner {
    pub signal: bool,
    pub waker: WakerRegistration,
}

impl ChannelStateInner {
    pub const fn new() -> Self {
        Self {
            signal: false,
            waker: WakerRegistration::new(),
        }
    }
}

pub struct ChannelState<C: Interrupt> {
    _marker: PhantomData<C>,
    inner: UnsafeCell<ChannelStateInner>,
}

impl<C> ChannelState<C>
where C: Interrupt {
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData{},
            inner: UnsafeCell::new(ChannelStateInner::new()),
        }
    }

    fn with<F, R>(&self, f: F) -> R
    where F: FnOnce(&mut ChannelStateInner) -> R
    {
        use embassy_cortex_m::interrupt::InterruptExt;
        let irq = unsafe { C::steal() };
        irq.disable();
        let r = f(unsafe { &mut *self.inner.get() });
        irq.enable();
        r
    }

    fn interrupt(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        inner.signal = true;
        inner.waker.wake();
    }
}

unsafe impl<C> Sync for ChannelState<C> where C: Interrupt {}

// impl<C> PeripheralState for ChannelState<C>
// where C: Channel
// {
//     type Interrupt = C::Interrupt;

//     fn on_interrupt(&mut self) {
//         let regs = C::Instance::regs();
//         let ch_num = C::number();

//         let intf = regs.intf.read();
//         if intf.errif0().bit_is_set() {
//             error!("DMA0_CHANNEL0: error");
//         }

//         let all_if = 0x0F_u32 << (4 * ch_num);
//         regs.intc.write(|w| unsafe { w.bits(all_if) });

//         self.signal.store(true, atomic_polyfill::Ordering::Relaxed);
//         self.waker.wake();
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    TransferError,
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

/// Read from a peripheral device. The `src` address is the memory-mapped peripheral register address to read from.
/// The `dest` address is the memory buffer to write to.
pub fn read<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, src: *const S, dest: *mut D, count: u16) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    read_inner(ch, src, dest, count, crate::pac::dma0::ch0ctl::PNAGA_A::INCREMENT)
}

/// Read from a peripheral device and repeatly write to the same dest address. The `src` address is the memory-mapped 
/// peripheral register address to read from. The `dest` address will be repeatally written to.
pub fn read_repeated<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, src: *const S, dest: *mut D, count: u16) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    read_inner(ch, src, dest, count, crate::pac::dma0::ch0ctl::PNAGA_A::FIXED)
}

fn read_inner<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, src: *const S, dest: *mut D, count: u16, mnaga: crate::pac::dma0::ch0ctl::PNAGA_A) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    C::Instance::enable();
    let mut ctrl_reg_val = 0;
    let ctrl_reg = unsafe { &*((&mut ctrl_reg_val) as *mut _ as *mut crate::pac::dma0::CH0CTL) };

    ctrl_reg.write(|w| {
        w.mwidth()
            .variant(D::width().into_p())
            .pwidth()
            .variant(S::width().into_p())
            .mnaga().variant(mnaga)
            .dir()
            .variant(crate::pac::dma0::ch0ctl::DIR_A::FROM_PERIPHERAL)
            .ftfie()
            .set_bit()
            .chen()
            .set_bit()
    });

    unsafe {
        C::state().with(|inner| {
            inner.signal = false;
            inner.waker = WakerRegistration::new();
            configure_channel(
                C::Instance::regs(),
                C::number(),
                dest as *const (),
                src as *const (),
                ctrl_reg_val,
                count,
            );
        });
        
    }

    into_ref!(ch);
    Transfer::new(ch)
}

/// Write to a peripheral device. The `src` address should be the memory buffer to read from. The `dest` should be the 
/// memory-mapped peripheral register address to write to.
pub fn write<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, src: *const S, dest: *mut D, count: u16) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    write_inner(ch, src, dest, count, crate::pac::dma0::ch0ctl::PNAGA_A::INCREMENT)
}

/// Repeatably write the same value to a peripheral device. The `dest` should be the 
/// memory-mapped peripheral register address to write to.
pub fn write_repeated<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, value: S, dest: *mut D, count: u16) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    let src = [value];
    write_inner(ch, src.as_ptr(), dest, count, crate::pac::dma0::ch0ctl::PNAGA_A::FIXED)
}

fn write_inner<'a, C: Channel, S, D>(ch: PeripheralRef<'a, C>, src: *const S, dest: *mut D, count: u16, mnaga: crate::pac::dma0::ch0ctl::PNAGA_A) -> Transfer<'a, C>
where
    C: Channel,
    S: Word,
    D: Word,
{
    C::Instance::enable();
    let mut ctrl_reg_val = 0;

    let ctrl_reg = unsafe { &*(&mut ctrl_reg_val as *mut _ as *mut crate::pac::dma0::CH0CTL) };

    ctrl_reg.write(|w| {
        w.mwidth()
            .variant(S::width().into_p())
            .pwidth()
            .variant(D::width().into_p())
            .mnaga().variant(mnaga)
            .dir()
            .variant(crate::pac::dma0::ch0ctl::DIR_A::FROM_MEMORY)
            .ftfie()
            .set_bit()
            .chen()
            .set_bit()
    });

    unsafe {

        C::state().with(|inner| {
            inner.signal = false;
            inner.waker = WakerRegistration::new();
            configure_channel(
                C::Instance::regs(),
                C::number(),
                src as *const (),
                dest as *const (),
                ctrl_reg_val,
                count,
            );
        });
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

        let channel_state = C::state();
        channel_state.with(|inner| {
            if inner.signal {
                Poll::Ready(Ok(()))
            } else {
                inner.waker.register(cx.waker());
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

pub trait Instance: Peripheral<P = Self> + crate::cctl::CCTLPeripherial + 'static {
    //fn wakers() -> &'static [waker::DmaWaker];

    fn regs() -> &'static crate::pac::dma0::RegisterBlock;
}

pub trait Channel: Peripheral<P = Self> + 'static {
    type Instance: Instance;
    type Interrupt: crate::interrupt::Interrupt;

    fn number() -> u8;
    fn state() -> &'static ChannelState<Self::Interrupt>;
}

macro_rules! impl_dma {
    ($type:ident, $pac_type:ident, $num_channels:expr) => {
        impl crate::dma::Instance for peripherals::$type {
            // fn wakers() -> &'static [crate::dma::waker::DmaWaker] {
            //     const NEW_AW: crate::dma::waker::DmaWaker = crate::dma::waker::DmaWaker::new();
            //     static WAKERS: [crate::dma::waker::DmaWaker; $num_channels] = [NEW_AW; $num_channels];
            //     &WAKERS
            // }

            fn regs() -> &'static crate::pac::dma0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::dma0::RegisterBlock) }
            }
        }
    };
}

macro_rules! impl_dma_channel {
    ($name:ident, $inst:ident, $ch:expr, $irq:ident) => {
        impl crate::dma::Channel for peripherals::$name {
            type Instance = peripherals::$inst;
            type Interrupt = crate::interrupt::$irq;

            fn number() -> u8 {
                $ch
            }

            fn state() -> &'static crate::dma::ChannelState<crate::interrupt::$irq> {
                static STATE: crate::dma::ChannelState<crate::interrupt::$irq> = crate::dma::ChannelState::new();
                &STATE
            }
        }
    };
}

impl crate::cctl::CCTLPeripherial for crate::peripherals::DMA0 {
    fn frequency() -> crate::utils::Hertz {
        let clocks = crate::cctl::get_freq();
        clocks.ahb
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.ahben.modify(|_, w| w.dma0en().set_bit() );
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.ahben.modify(|_, w| w.dma0en().clear_bit() );
    }
}

impl crate::cctl::CCTLPeripherial for crate::peripherals::DMA1 {
    fn frequency() -> crate::utils::Hertz {
        let clocks = crate::cctl::get_freq();
        clocks.ahb
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.ahben.modify(|_, w| w.dma0en().set_bit() );
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.ahben.modify(|_, w| w.dma0en().clear_bit() );
    }
}

#[interrupt]
unsafe fn DMA0_CHANNEL0() {
    //debug!("DMA0_CHANNEL0");
    let mut inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif0().bit_is_set() {
        error!("DMA0_CHANNEL0: error");
    }

    let all_if = 0x0F_u32 << (4 * 0);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH0::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL1() {
    //debug!("DMA0_CHANNEL1");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif1().bit_is_set() {
        error!("DMA0_CHANNEL1: error");
    }

    let all_if = 0x0F_u32 << (4 * 1);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH1::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL2() {
    //debug!("DMA0_CHANNEL2");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif2().bit_is_set() {
        error!("DMA0_CHANNEL2: error");
    }

    let all_if = 0x0F_u32 << (4 * 2);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH2::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL3() {
    //debug!("DMA0_CHANNEL3");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif3().bit_is_set() {
        error!("DMA0_CHANNEL3: error");
    }

    let all_if = 0x0F_u32 << (4 * 3);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH3::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL4() {
    //debug!("DMA0_CHANNEL4");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif4().bit_is_set() {
        error!("DMA0_CHANNEL4: error");
    }

    let all_if = 0x0F_u32 << (4 * 4);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH4::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL5() {
    debug!("DMA0_CHANNEL5");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif5().bit_is_set() {
        error!("DMA0_CHANNEL5: error");
    }

    let all_if = 0x0F_u32 << (4 * 5);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH5::state().interrupt();
}

#[interrupt]
unsafe fn DMA0_CHANNEL6() {
    debug!("DMA0_CHANNEL6");
    let inst = &*crate::pac::DMA0::ptr();
    let intf = inst.intf.read();
    if intf.errif6().bit_is_set() {
        error!("DMA0_CHANNEL6: error");
    }

    let all_if = 0x0F_u32 << (4 * 6);
    inst.intc.write(|w| unsafe { w.bits(all_if) });

    crate::chip::peripherals::DMA0_CH6::state().interrupt();
}
