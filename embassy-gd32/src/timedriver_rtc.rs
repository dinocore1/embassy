use core::borrow::BorrowMut;
use core::cell::{RefCell, UnsafeCell};

use crate::interrupt::{Interrupt, InterruptExt};
use crate::{interrupt, pac};
//use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::CriticalSectionMutex;
//use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

fn rtc() -> &'static crate::pac::rtc::RegisterBlock {
    unsafe { &*crate::pac::RTC::ptr() }
}

struct RtcDriver {
    state: CriticalSectionMutex<UnsafeCell<RtcState>>,
}

struct RtcState {
    /// Number of 2^32 periods elapsed since boot.
    period: u32,
    last_read_value: u32,
}

impl RtcState {

    fn read_time(&mut self) -> u64 {

        let r = rtc();

        let mut overflow = false;
        r.ctl.modify(|r, w| {
            overflow = r.ovif().bit_is_set();
            w.ovif().clear_bit()
        });

        let counter = read_counter(r);

        if overflow || counter < self.last_read_value {
            self.period += 1;
        }

        self.last_read_value = counter;

        let time = (self.period as u64) << 32;
        let time = time | counter as u64;
        time
    }
}

#[inline]
fn read_counter(r: &crate::pac::rtc::RegisterBlock) -> u32 {
    (r.cnth.read().cnt().bits() as u32) << 16 | 
     r.cntl.read().cnt().bits() as u32
}

impl RtcDriver {

    fn init(&'static self) {
        let r = rtc();

        let irq = unsafe { interrupt::RTC::steal() };


        //wait for the LWOFF bit is 1
        while r.ctl.read().lwoff().bit_is_clear() {}

        //enter config mode
        r.ctl.modify(|_, w| w.cmf().set_bit());

        //enable only the overflow interrupt
        r.inten.write(|w| w.
            ovie().set_bit()
        );

        //exit config mode
        r.ctl.modify(|_, w| w.cmf().clear_bit());

        //wait for the LWOFF bit is 1
        while r.ctl.read().lwoff().bit_is_clear() {}

    }

    fn on_interrupt(&self) {
        let r = rtc();

        self.state.lock(|s| {
            let state = s.get_mut();
            
            state.read_time();

        });

    }

}

unsafe impl Sync for RtcDriver {
}

impl embassy_time::driver::Driver for RtcDriver {
    fn now(&self) -> u64 {

        self.state.lock(|s| {
            let state = s.get_mut();
            state.read_time()
        })
    }

    unsafe fn allocate_alarm(&self) -> Option<embassy_time::driver::AlarmHandle> {
        todo!()
    }

    fn set_alarm_callback(&self, alarm: embassy_time::driver::AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
        todo!()
    }

    fn set_alarm(&self, alarm: embassy_time::driver::AlarmHandle, timestamp: u64) -> bool {
        todo!()
    }
}

embassy_time::time_driver_impl!(static DRIVER: RtcDriver = RtcDriver {
    state: Mutex::const_new(CriticalSectionRawMutex::new(), RtcState {
        period: 0,
        last_read_value: 0,
    }),
});

#[interrupt]
fn RTC() {
    DRIVER.on_interrupt();
}

pub(crate) fn init() {
    DRIVER.init();
}