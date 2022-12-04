#![no_std]

#[cfg(not(any(
    feature = "gd32e503",
)))]
compile_error!("No chip feature activated. You must activate one of the chip features.");


cfg_if::cfg_if! {
    if #[cfg(feature = "gd32e503")] {
        //pub use gd32e5::
    }
}

pub use embassy_cortex_m::executor;
