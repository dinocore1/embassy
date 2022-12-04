#![no_std]

cfg_if::cfg_if! {
    if #[cfg(feature = "gd32e503")] {
        //pub use gd32e5::
    }
}
