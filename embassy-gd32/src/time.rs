use core::ops::{Div, Mul};


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

pub trait ClockDivider {
    fn divide(&self, hz: Hertz) -> Hertz;   
}

pub trait ClockMultiplier {
    fn multiply(&self, hz: Hertz) -> Hertz;
}

impl<D> Div<D> for Hertz
where D: ClockDivider {
    type Output = Hertz;

    fn div(self, rhs: D) -> Self::Output {
        rhs.divide(self)
    }
}

impl<M> Mul<M> for Hertz
where M: ClockMultiplier {
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
        todo!()
    }
}