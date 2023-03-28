use core::{ops::Range, ptr::write_volatile};

use embassy_hal_common::{into_ref, PeripheralRef};

use crate::{peripherals, Peripheral};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    PageNotAligned(u32),
}

#[cfg(feature = "nightly")]
impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

pub struct Flash<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
}

impl<'d, T: Instance> Flash<'d, T> {

    pub const PAGE_SIZE: usize = 8 * 1024;
    pub const WRITE_SIZE: usize = 4;

    pub fn new(p: impl Peripheral<P = T> + 'd) -> Self {
        into_ref!(p);
        Self { _p: p }
    }

    pub fn unlock(&self) {
        let regs = T::regs();
        regs.key.write(|w| w.bits(0x45670123));
        regs.key.write(|w| w.bits(0xCDEF89AB));
    }

    pub fn lock(&self) {
        let regs = T::regs();
        regs.ctl.modify(|_, w| w.lk().set_bit());
    }

    pub fn blocking_erase(&self, page_range: Range<u32>) -> Result<(), Error> {
        let regs = T::regs();

        if !Self::is_page_aligned(page_range.start) {
            return Err(Error::PageNotAligned(page_range.start));
        }

        for page in page_range.step_by(Self::PAGE_SIZE) {
            while regs.stat.read().busy().bit_is_set() {}
            regs.addr.write(|w| w.addr().variant(page) );
            regs.ctl.modify(|_, w| w
                .per().set_bit()
                .start().set_bit()
            );
        }

        while regs.stat.read().busy().bit_is_set() {}
        regs.ctl.modify(|_, w| w.per().clear_bit());
        Ok(())
    }

    pub fn blocking_write(&self, mut addr: u32, buf: &[u8]) -> Result<(), Error> {
        let regs = T::regs();

        for chunk in buf.chunks(Self::WRITE_SIZE) {
            while regs.stat.read().busy().bit_is_set() {}
            regs.ctl.modify(|_, w| w.pg().set_bit());
            unsafe { write_volatile(addr as *mut u32, u32::from_le_bytes(chunk.try_into().unwrap())) };
            addr += Self::WRITE_SIZE as u32;
        }

        while regs.stat.read().busy().bit_is_set() {}
        
        Ok(())
    }

    fn is_page_aligned(address: u32) -> bool {
        address % Self::PAGE_SIZE as u32 == 0
    }

}

impl<'d, T: Instance> Drop for Flash<'d, T> {
    fn drop(&mut self) {
        self.lock();
    }
}

pub(crate) mod sealed {
    pub trait Instance {
        fn regs() -> &'static crate::pac::fmc::RegisterBlock;
    }
}

pub trait Instance: Peripheral<P = Self> + sealed::Instance + 'static {}

impl Instance for peripherals::FMC {}
impl sealed::Instance for peripherals::FMC {
    fn regs() -> &'static crate::pac::fmc::RegisterBlock {
        unsafe { &*crate::pac::FMC::ptr() }
    }
}
