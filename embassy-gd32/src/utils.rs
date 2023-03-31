use core::{ops::{Div, Mul}};

#[derive(PartialEq, PartialOrd, Clone, Copy, Debug, Eq)]
pub struct Hertz(pub u32);

impl Hertz {
    pub fn hz(hertz: u32) -> Self {
        Self(hertz)
    }

    pub fn khz(kilohertz: u32) -> Self {
        Self(kilohertz * 1_000)
    }

    pub fn mhz(megahertz: u32) -> Self {
        Self(megahertz * 1_000_000)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Hertz {
    fn format(&self, fmt: defmt::Formatter) {
        if self.0 < 1_000 {
            defmt::write!(fmt, "{}hz", self.0);
        } else if self.0 < 1_000_000 {
            defmt::write!(fmt, "{}Khz", self.0 / 1_000);
        } else {
            defmt::write!(fmt, "{}Mhz", self.0 / 1_000_000);
        }
    }
}

pub trait ClockDivider {
    fn divide(&self, hz: Hertz) -> Hertz;
}

pub trait ClockMultiplier {
    fn multiply(&self, hz: Hertz) -> Hertz;
}

impl<D> Div<D> for Hertz
where
    D: ClockDivider,
{
    type Output = Hertz;

    fn div(self, rhs: D) -> Self::Output {
        rhs.divide(self)
    }
}

impl<M> Mul<M> for Hertz
where
    M: ClockMultiplier,
{
    type Output = Hertz;

    fn mul(self, rhs: M) -> Self::Output {
        rhs.multiply(self)
    }
}

impl ClockMultiplier for u32 {
    fn multiply(&self, hz: Hertz) -> Hertz {
        Hertz::hz(hz.0 * self)
    }
}

impl ClockDivider for u32 {
    fn divide(&self, hz: Hertz) -> Hertz {
        Hertz::hz(hz.0 / self)
    }
}

impl AsRef<u32> for Hertz {
    fn as_ref(&self) -> &u32 {
        &self.0
    }
}

// #[inline(always)]
// unsafe fn very_bad_function<T>(reference: &T) -> &mut T {
//     let const_ptr = reference as *const T;
//     let mut_ptr = const_ptr as *mut T;
//     &mut *mut_ptr
// }

// pub struct InterruptWaker {
//     waker: Option<Waker>,
// }

// impl InterruptWaker {
//     pub const fn new() -> Self {
//         Self {
//             waker: None,
//         }
//     }

//     pub fn signal(&self) {
//         let waker = critical_section::with(|_cs| {
//             let this = unsafe { very_bad_function(self) };
//             this.waker.take()
//         });
//         if let Some(waker) = waker {
//             waker.wake();
//         }
//     }

//     pub fn register(&self, cx: &mut Context, _cs: CriticalSection) {
//         let this = unsafe { very_bad_function(self) };

//         if let Some(waker) = this.waker.take() {
//             if !waker.will_wake(cx.waker()) {
//                 // two different threads are waiting on this. Two threads will fight
//                 // but at least functionality will be correct.
//                 waker.wake();
//             }
//         }
//         this.waker = Some(cx.waker().clone());
//     }
// }
