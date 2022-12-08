#![no_std]

#[cfg(not(any(
    feature = "gd32e503",
)))]
compile_error!("No chip feature activated. You must activate one of the chip features.");



#[cfg_attr(feature = "gd32e503", path = "chips/gd32e503.rs")]
mod chip;

pub use chip::pac;

mod time;
mod cctl;

pub use cctl::*;

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

pub fn init(config: Config) -> chip::pac::Peripherals {

    let peripherals = chip::pac::Peripherals::take().unwrap();

    cctl::init(&peripherals.RCU, &config.clock_cfg);
    

    peripherals

}
