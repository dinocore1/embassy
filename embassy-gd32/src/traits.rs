#![macro_use]

macro_rules! pin_trait {
    ($signal:ident, $instance:path) => {
        pub trait $signal<T: $instance>: crate::gpio::Pin {
        }
    };
}

macro_rules! pin_trait_impl {
    (crate::$mod:ident::$trait:ident, $instance:ident, $pin:ident) => {
        impl crate::$mod::$trait<crate::peripherals::$instance> for crate::peripherals::$pin {
        }
    };
}