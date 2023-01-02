use core::cell::{Cell, UnsafeCell};
use core::ptr;
use core::sync::atomic::{AtomicU8, Ordering};

use critical_section::CriticalSection;
use defmt::{info, unwrap};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::CriticalSectionMutex as Mutex;
use embassy_time::driver::{AlarmHandle, Driver};

use crate::interrupt;
use crate::interrupt::{Interrupt, InterruptExt};

fn rtc() -> &'static crate::pac::rtc::RegisterBlock {
    unsafe { &*crate::pac::RTC::ptr() }
}

const ALARM_COUNT: usize = 3;

struct RtcDriver {
    state: Mutex<UnsafeCell<RtcState>>,
    alarm_count: AtomicU8,
    alarms: Mutex<[AlarmState; ALARM_COUNT]>,
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
    (r.cnth.read().cnt().bits() as u32) << 16 | r.cntl.read().cnt().bits() as u32
}

fn do_config<F>(f: F)
where
    F: FnOnce(),
{
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
        rcu.apb1en.modify(|_, w| w.pmuen().set_bit().bkpien().set_bit());

        //set backup domain write enable so we can turn on lxt and rtc
        pmu.ctl0.write(|w| w.bkpwen().set_bit());

        //set the rtc clock mux to lxtal and enable lxtal
        rcu.bdctl.write(|w| w.rtcsrc().variant(0b01).lxtalen().set_bit());

        //wait for lxtal to become stable
        while rcu.bdctl.read().lxtalstb().bit_is_clear() {}

        //enable rtc clock
        rcu.bdctl.modify(|_, w| w.rtcen().set_bit());

        do_config(|| {
            // set the counter to zero
            r.cnth.write(|w| w.cnt().variant(0));
            r.cntl.write(|w| w.cnt().variant(0));

            //set the prescaler to zero
            r.psch.write(|w| w.psc().variant(0));
            r.pscl.write(|w| w.psc().variant(0));

            //enable overflow and alarm interrupt
            r.inten.write(|w| w.ovie().set_bit().alrmie().set_bit());
        });

        let irq = unsafe { interrupt::RTC::steal() };
        irq.set_priority(crate::interrupt::Priority::P1);
        irq.enable();
    }

    fn on_interrupt(&self) {
        let rtc = rtc();

        let mut alarm = false;
        rtc.ctl.modify(|r, w| {
            alarm = r.alrmif().bit_is_set();
            w.alrmif().clear_bit()
        });

        critical_section::with(|cs| {
            let state = unsafe { &mut *self.state.borrow(cs).get() };
            let alarms = self.alarms.borrow(cs);

            let now = state.read_time();

            for a in &alarms[0..self.alarm_count.load(Ordering::Acquire) as usize] {
                if a.timestamp.get() <= now {
                    //trigger the alarm
                    a.timestamp.set(u64::MAX);
                    let f: fn(*mut ()) = unsafe { core::mem::transmute(a.callback.get()) };
                    f(a.ctx.get());
                }
            }

            self.reset_alarm(cs);
        });
    }

    fn get_alarm<'a>(&'a self, cs: CriticalSection<'a>, alarm: AlarmHandle) -> &'a AlarmState {
        // safety: we're allowed to assume the AlarmState is created by us, and
        // we never create one that's out of bounds.
        unsafe { self.alarms.borrow(cs).get_unchecked(alarm.id() as usize) }
    }

    fn get_next<'a>(&'a self, cs: CriticalSection<'a>) -> u64 {
        let alarms = self.alarms.borrow(cs);
        let mut min = u64::MAX;
        for a in &alarms[0..self.alarm_count.load(Ordering::Acquire) as usize] {
            min = min.min(a.timestamp.get());
        }
        min
    }

    fn reset_alarm<'a>(&'a self, cs: CriticalSection<'a>) {
        let rtc = rtc();
        let next_timestamp = self.get_next(cs);

        let alarm_value = (0x0000_0000_FFFF_FFFF & next_timestamp) as u32;
        do_config(|| {
            rtc.alrmh.write(|w| w.alrm().variant((alarm_value >> 16) as u16));
            rtc.alrml.write(|w| w.alrm().variant((0xFFFF & alarm_value) as u16));
        });
    }
}

unsafe impl Sync for RtcDriver {}

impl Driver for RtcDriver {
    fn now(&self) -> u64 {
        self.state.lock(|s| {
            let state = unsafe { &mut *s.get() };
            state.read_time()
        })
    }

    unsafe fn allocate_alarm(&self) -> Option<embassy_time::driver::AlarmHandle> {
        let id = self.alarm_count.fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
            if x < ALARM_COUNT as u8 {
                Some(x + 1)
            } else {
                None
            }
        });

        match id {
            Ok(id) => Some(AlarmHandle::new(id)),
            Err(_) => None,
        }
    }

    fn set_alarm_callback(&self, alarm: embassy_time::driver::AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
        critical_section::with(|cs| {
            let alarm = self.get_alarm(cs, alarm);
            alarm.callback.set(callback as *const ());
            alarm.ctx.set(ctx);
        })
    }

    fn set_alarm(&self, alarm: embassy_time::driver::AlarmHandle, timestamp: u64) -> bool {
        let now = self.now();
        // if the alarm is in the past
        if timestamp <= now {
            return false;
        }

        critical_section::with(|cs| {
            let alarm = self.get_alarm(cs, alarm);
            alarm.timestamp.set(timestamp);

            self.reset_alarm(cs);
        });

        true
    }
}

const ALARM_STATE_NEW: AlarmState = AlarmState::new();

embassy_time::time_driver_impl!(static DRIVER: RtcDriver = RtcDriver {
    state: Mutex::const_new(CriticalSectionRawMutex::new(), UnsafeCell::new(RtcState {
        period: 0,
        last_read_value: 0,
    })),
    alarm_count: AtomicU8::new(0),
    alarms: Mutex::const_new(CriticalSectionRawMutex::new(), [ALARM_STATE_NEW ; ALARM_COUNT]),
});

#[interrupt]
fn RTC() {
    DRIVER.on_interrupt();
}

pub(crate) fn init() {
    DRIVER.init();
}
