//! Timedriver using the Cortex-M SysTick timer
use core::{
    cell::UnsafeCell,
    sync::atomic::Ordering,
    ptr,
    slice,
};
use atomic_polyfill::{AtomicU64, AtomicU8};
use embassy_time::driver::{Driver, AlarmHandle};

const ALARM_COUNT: usize = 4;

struct SystickDriver{
    ts: AtomicU64,
    alarm_count: AtomicU8,
    alarms: UnsafeCell<[AlarmState ; ALARM_COUNT]>,
}

impl SystickDriver {
    const fn new() -> Self {
        SystickDriver {
            ts: AtomicU64::new(0),
            alarm_count: AtomicU8::new(0),
            alarms: UnsafeCell::new([ALARM_NEW ; ALARM_COUNT]),
        }
    }

    unsafe fn alarms(&self) -> &mut [AlarmState] {
        let alarms_ptr = &mut *self.alarms.get();
        let alarms_ptr = alarms_ptr.as_mut_ptr();
        let alarms = slice::from_raw_parts_mut(alarms_ptr, self.alarm_count.load(Ordering::Relaxed) as usize);
        alarms
    }

    fn lock<F>(&self, f: F)
    where F: FnOnce(&mut [AlarmState]) {
        let p = unsafe { cortex_m::Peripherals::steal() };
        let mut syst = p.SYST;
        let alarms = unsafe { self.alarms() };

        syst.disable_interrupt();
        f(alarms);
        syst.enable_interrupt();

    }
}

unsafe impl Send for SystickDriver{}
unsafe impl Sync for SystickDriver{}

struct AlarmState {
    ts: u64,
    callback: *const (),
    ctx: *mut (),
}

impl AlarmState {
    const fn new() -> Self {
        Self {
            ts: u64::MAX,
            callback: ptr::null(),
            ctx: ptr::null_mut(),
        }
    }

    fn trigger(&mut self) {
        self.ts = u64::MAX;
        let f: fn(*mut()) = unsafe { core::mem::transmute(self.callback) };
        f(self.ctx);
    }
}

unsafe impl Send for AlarmState{}
unsafe impl Sync for AlarmState{}

const ALARM_NEW: AlarmState = AlarmState::new();

embassy_time::time_driver_impl!(static DRIVER: SystickDriver = SystickDriver::new());

impl Driver for SystickDriver {

    fn now(&self) -> u64 {
        self.ts.load(Ordering::Relaxed)
    }

    unsafe fn allocate_alarm(&self) -> Option<AlarmHandle> {
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

    fn set_alarm_callback(&self, alarm: AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
        self.lock(|alarms| {
            let alarm = &mut alarms[alarm.id() as usize];
            alarm.callback = callback as *const ();
            alarm.ctx = ctx;
        });
    }

    fn set_alarm(&self, alarm: AlarmHandle, timestamp: u64) -> bool {
        let now = self.now();
        if timestamp <= now {
            false
        } else {
            let alarms = unsafe { &mut *self.alarms.get() };
            let alarm = &mut alarms[alarm.id() as usize];
            alarm.ts = timestamp;
            true
        }
    }

}

/// initialize the SysTick Timedriver. 
pub fn init(cpu_hertz: u64) {
    let f = cpu_hertz / embassy_time::TICK_HZ;
    let f = f as u32;

    let mut p = unsafe { cortex_m::Peripherals::steal() };
    p.SYST.set_reload(f - 1);
    p.SYST.clear_current();
    p.SYST.enable_counter();
    p.SYST.enable_interrupt();
}

/// Call this function from the SysTick exception handler
#[no_mangle]
pub extern "C" fn systick_timedriver_interrupt() {
    let ts = DRIVER.ts.fetch_add(1, Ordering::Release);
    let alarms = unsafe { DRIVER.alarms() };
    for alarm in alarms {
        if alarm.ts <= ts {
            alarm.trigger();
        }
    }
}