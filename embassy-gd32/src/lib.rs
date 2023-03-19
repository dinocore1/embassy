#![no_std]

#![cfg_attr(
    feature = "nightly",
    feature(async_fn_in_trait)
)]

#[cfg(not(any(feature = "gd32e503",)))]
compile_error!("No chip feature activated. You must activate one of the chip features.");

// This mod MUST go first, so that the others see its macros.
pub(crate) mod fmt;

mod traits;

mod utils;
pub use utils::Hertz;

pub mod cctl;

pub mod gpio;

pub mod spi;

pub mod dma;

pub mod usart;

pub mod fmc;

#[cfg_attr(feature = "gd32e503", path = "chips/gd32e503.rs")]
mod chip;
pub(crate) use chip::pac;
pub use chip::{peripherals, Peripherals};

pub mod interrupt {
    pub use cortex_m::interrupt::{CriticalSection, Mutex};
    pub use embassy_cortex_m::interrupt::*;

    pub use crate::chip::irqs::*;
}

#[cfg(feature = "timedriver-rtc")]
mod timedriver_rtc;

pub use embassy_cortex_m::executor;
pub use embassy_cortex_m::interrupt::_export::interrupt;
pub use embassy_hal_common::{into_ref, Peripheral, PeripheralRef};
#[cfg(feature = "timedriver-rtc")]
pub use embassy_time::*;

pub use cortex_m;

pub struct Config {
    pub clock_cfg: cctl::Config,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clock_cfg: cctl::Config::default(),
        }
    }
}

pub fn init(config: Config) -> chip::Peripherals {
    let peripherals = chip::pac::Peripherals::take().unwrap();

    cctl::init(&peripherals.RCU, &peripherals.FMC, &config.clock_cfg);

    #[cfg(feature = "timedriver-rtc")]
    timedriver_rtc::init();

    chip::Peripherals::take()
}
