
use core::ptr;
use core::sync::atomic::{Ordering, AtomicBool};
use core::cell::{UnsafeCell, Cell};

use crate::interrupt::{Interrupt, InterruptExt};
use crate::{interrupt};
use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::driver::AlarmHandle;

use embassy_time::driver::Driver;

use defmt::{info, unwrap};

fn rtc() -> &'static crate::pac::rtc::RegisterBlock {
    unsafe { &*crate::pac::RTC::ptr() }
}

struct RtcDriver {
    state: CriticalSectionMutex<UnsafeCell<RtcState>>,
    alarm_free: AtomicBool,
    alarm_state: AlarmState,
}

struct AlarmState {
    timestamp: Cell<u64>,
    callback: Cell<*const ()>,
    ctx: Cell<*mut ()>,
}

unsafe impl Send for AlarmState {}

impl AlarmState {

    const fn new() -> Self {
        Self {
            timestamp: Cell::new(u64::MAX),
            callback: Cell::new(ptr::null()),
            ctx: Cell::new(ptr::null_mut()),
        }
    }
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

        if overflow {
            self.period += 1;
        }

        let counter = read_counter(r);
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

fn do_config<F>(f: F)
where F: FnOnce() {
    let r = rtc();

    //wait for the LWOFF bit is 1
    while r.ctl.read().lwoff().bit_is_clear() {}

    //enter config mode
    r.ctl.modify(|_, w| w.cmf().set_bit());

    f();

    //exit config mode
    r.ctl.modify(|_, w| w.cmf().clear_bit());

    //wait for the LWOFF bit is 1
    while r.ctl.read().lwoff().bit_is_clear() {}

}

impl RtcDriver {

    fn init(&'static self) {
        let r = rtc();

        let rcu = unsafe { &*crate::pac::RCU::ptr() };
        let pmu = unsafe { &*crate::pac::PMU::ptr() };

        //enable power control and backup interface clocks
        rcu.apb1en.modify(|_, w| w.pmuen().set_bit()
                                           .bkpien().set_bit()
                                        );
        
        //set backup domain write enable so we can turn on lxt and rtc
        pmu.ctl0.write(|w| w.bkpwen().set_bit() );

        //set the rtc clock mux to lxtal and enable lxtal
        rcu.bdctl.write(|w| w.rtcsrc().variant(0b01)
                                        .lxtalen().set_bit()
                                    );

        //wait for lxtal to become stable
        while rcu.bdctl.read().lxtalstb().bit_is_clear() {}

        //enable rtc clock
        rcu.bdctl.modify(|_, w| w.rtcen().set_bit());

        do_config(|| {

            // set the counter to zero
            r.cnth.write(|w| w.cnt().variant(0));
            r.cntl.write(|w| w.cnt().variant(0));

            //set the prescaler to zero
            r.psch.write(|w|w.psc().variant(0));
            r.pscl.write(|w|w.psc().variant(0));

            //enable only the overflow interrupt
            r.inten.write(|w| w.
                ovie().set_bit()
            );
            
        });

        let irq = unsafe { interrupt::RTC::steal() };
        irq.set_priority(crate::interrupt::Priority::P1);
        irq.enable();

    }

    fn on_interrupt(&self) {
        let rtc = rtc();

        self.state.lock(|s| {
            let state = unsafe { &mut *s.get() };

            do_config(|| {
                rtc.inten.modify(|_, w| w.alrmie().clear_bit());
            });
            
            let now = state.read_time();

            let r = self.alarm_free.fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
                if !x && self.alarm_state.timestamp.get() <= now {
                    Some(true)
                } else {
                    None
                }
            });

            if let Ok(_) = r {
                let f: fn(*mut()) = unsafe { core::mem::transmute(self.alarm_state.callback.get()) };
                f(self.alarm_state.ctx.get());
            }

        });

    }

}

unsafe impl Sync for RtcDriver {
}

impl Driver for RtcDriver {
    fn now(&self) -> u64 {
        self.state.lock(|s| {
            let state = unsafe { &mut *s.get() };
            state.read_time()
        })
    }

    unsafe fn allocate_alarm(&self) -> Option<embassy_time::driver::AlarmHandle> {
        let id = self.alarm_free.fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
            if x {
                Some(false)
            } else {
                None
            }
        });

        match id {
            Ok(_) => Some(AlarmHandle::new(0)),
            Err(_) => None,
        }
    }

    fn set_alarm_callback(&self, alarm: embassy_time::driver::AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
        self.alarm_state.callback.set(callback as *const());
        self.alarm_state.ctx.set(ctx);
    }

    fn set_alarm(&self, alarm: embassy_time::driver::AlarmHandle, timestamp: u64) -> bool {
        let t = self.now();
        if timestamp <= t {
            return false;
        }
        
        self.alarm_state.timestamp.set(timestamp);
        let alarm_value = (0x0000_0000_FFFF_FFFF & timestamp) as u32;


        do_config(||{
            let r = rtc();
            r.alrmh.write(|w| w.alrm().variant((alarm_value >> 16) as u16));
            r.alrml.write(|w| w.alrm().variant((0xFFFF & alarm_value) as u16));
            r.inten.modify(|_, w| w.alrmie().set_bit());
        });

        true

    }
}

embassy_time::time_driver_impl!(static DRIVER: RtcDriver = RtcDriver {
    state: CriticalSectionMutex::const_new(CriticalSectionRawMutex::new(), UnsafeCell::new(RtcState {
        period: 0,
        last_read_value: 0,
    })),
    alarm_free: AtomicBool::new(true),
    alarm_state: AlarmState::new(),
});

#[interrupt]
fn RTC() {
    DRIVER.on_interrupt();
}

pub(crate) fn init() {
    DRIVER.init();
}