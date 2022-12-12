

fn rtc() -> &'static crate::pac::rtc::RegisterBlock {
    unsafe { &*crate::pac::RTC::ptr() }
}

struct RtcDriver {
    /// Number of 2^32 periods elapsed since boot.
    period: u32,
}

unsafe impl Sync for RtcDriver {
}

impl embassy_time::driver::Driver for RtcDriver {
    fn now(&self) -> u64 {

        critical_section::with(|_|{
            let period = self.period as u64;
            let counter = rtc().cnth.read().cnt().bits() as u64;
            (period << 32) | counter
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
    period: 0,
});