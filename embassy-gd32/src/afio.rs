use embassy_hal_common::Peripheral;

use crate::{peripherals, cctl::CCTLPeripherial};



pub trait Instance: Peripheral<P = Self> + 'static {
    fn regs() -> &'static crate::pac::afio::RegisterBlock;
}

impl Instance for peripherals::AFIO {
    fn regs() -> &'static crate::pac::afio::RegisterBlock {
        unsafe { &*crate::pac::AFIO::ptr() }
    }
}

impl CCTLPeripherial for crate::peripherals::AFIO {
    fn frequency() -> crate::utils::Hertz {
        let clocks = crate::cctl::get_freq();
        clocks.apb2
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.afen().set_bit() );
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.afen().clear_bit() );
    }
}