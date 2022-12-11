#![no_std]

#[cfg(not(any(
    feature = "gd32e503",
)))]
compile_error!("No chip feature activated. You must activate one of the chip features.");

mod time;

pub mod cctl;

pub mod gpio;

#[cfg_attr(feature = "gd32e503", path = "chips/gd32e503.rs")]
mod chip;
pub(crate) use chip::pac;


pub use embassy_hal_common::{into_ref, Peripheral, PeripheralRef};
pub use embassy_cortex_m::executor;

pub struct Config {
    clock_cfg: cctl::Config,
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

    gpio::init(&peripherals.RCU);
    

    chip::Peripherals::take()

}
