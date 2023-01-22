pub(crate) use gd32e5::gd32e503 as pac;

pub const FLASH_SIZE: usize = 512 * 1024;

use crate::gpio::{AnyPin, Pin};

embassy_hal_common::peripherals! {
    PMU,
    RTC,
    RCU,
    DMA0,
    DMA1,
    USART0,
    USART1,
    SPI0,
    SPI1,
    GPIOA,
    GPIOB,
    GPIOC,
    PA0,
    PA1,
    PA2,
    PA3,
    PA4,
    PA5,
    PA6,
    PA7,
    PA8,
    PA9,
    PA10,
    PA11,
    PA12,
    PA13,
    PA14,
    PA15,

    PB0,
    PB1,
    PB2,
    PB3,
    PB4,
    PB5,
    PB6,
    PB7,
    PB8,
    PB9,
    PB10,
    PB11,
    PB12,
    PB13,
    PB14,
    PB15,

    DMA0_CH0,
    DMA0_CH1,
    DMA0_CH2,
    DMA0_CH3,
    DMA0_CH4,
    DMA0_CH5,
    DMA0_CH6,

    DMA1_CH0,
    DMA1_CH1,
    DMA1_CH2,
    DMA1_CH3,
    DMA1_CH4,
}

impl_gpio!(GPIOA, crate::gpio::GPIOPort::A);
impl_gpio!(GPIOB, crate::gpio::GPIOPort::B);
impl_gpio!(GPIOC, crate::gpio::GPIOPort::C);

impl_pin!(PA0, 0, 0);
impl_pin!(PA1, 0, 1);
impl_pin!(PA2, 0, 2);
impl_pin!(PA3, 0, 3);
impl_pin!(PA4, 0, 4);
impl_pin!(PA5, 0, 5);
impl_pin!(PA6, 0, 6);
impl_pin!(PA7, 0, 7);
impl_pin!(PA8, 0, 8);
impl_pin!(PA9, 0, 9);
impl_pin!(PA10, 0, 10);
impl_pin!(PA11, 0, 11);
impl_pin!(PA12, 0, 12);
impl_pin!(PA13, 0, 13);
impl_pin!(PA14, 0, 14);
impl_pin!(PA15, 0, 15);

impl_pin!(PB0, 1, 0);
impl_pin!(PB1, 1, 1);
impl_pin!(PB2, 1, 2);
impl_pin!(PB3, 1, 3);
impl_pin!(PB4, 1, 4);
impl_pin!(PB5, 1, 5);
impl_pin!(PB6, 1, 6);
impl_pin!(PB7, 1, 7);
impl_pin!(PB8, 1, 8);
impl_pin!(PB9, 1, 9);
impl_pin!(PB10, 1, 10);
impl_pin!(PB11, 1, 11);
impl_pin!(PB12, 1, 12);
impl_pin!(PB13, 1, 13);
impl_pin!(PB14, 1, 14);
impl_pin!(PB15, 1, 15);

pin_trait_impl!(crate::spi::SckPin, SPI0, PA5);
pin_trait_impl!(crate::spi::MisoPin, SPI0, PA6);
pin_trait_impl!(crate::spi::MosiPin, SPI0, PA7);

pin_trait_impl!(crate::spi::SckPin, SPI1, PB13);
pin_trait_impl!(crate::spi::MisoPin, SPI1, PB14);
pin_trait_impl!(crate::spi::MosiPin, SPI1, PB15);

impl_spi!(SPI0, SPI0, SPI0);
impl_spi!(SPI1, SPI1, SPI1);

impl_dma!(DMA0, DMA0, 7);
impl_dma!(DMA1, DMA1, 5);

impl_dma_channel!(DMA0_CH0, DMA0, 0);
impl_dma_channel!(DMA0_CH1, DMA0, 1);
impl_dma_channel!(DMA0_CH2, DMA0, 2);
impl_dma_channel!(DMA0_CH3, DMA0, 3);
impl_dma_channel!(DMA0_CH4, DMA0, 4);
impl_dma_channel!(DMA0_CH5, DMA0, 5);
impl_dma_channel!(DMA0_CH6, DMA0, 6);

impl_dma_channel!(DMA1_CH0, DMA1, 0);
impl_dma_channel!(DMA1_CH1, DMA1, 1);
impl_dma_channel!(DMA1_CH2, DMA1, 2);
impl_dma_channel!(DMA1_CH3, DMA1, 3);
impl_dma_channel!(DMA1_CH4, DMA1, 4);

dma_trait_impl!(crate::spi::TxDma, SPI0, DMA0_CH2);
dma_trait_impl!(crate::spi::RxDma, SPI0, DMA0_CH1);

dma_trait_impl!(crate::spi::TxDma, SPI1, DMA0_CH4);
dma_trait_impl!(crate::spi::RxDma, SPI1, DMA0_CH3);

impl_usart!(USART0, USART0, USART0, USART0_WAKER);
impl_usart!(USART1, USART1, USART1, USART1_WAKER);

pin_trait_impl!(crate::usart::TxPin, USART1, PA2);
pin_trait_impl!(crate::usart::RxPin, USART1, PA3);

pin_trait_impl!(crate::usart::TxPin, USART0, PA9);
pin_trait_impl!(crate::usart::RxPin, USART0, PA10);
pin_trait_impl!(crate::usart::TxPin, USART0, PB6);
pin_trait_impl!(crate::usart::RxPin, USART0, PB7);

pub mod irqs {
    use embassy_cortex_m::interrupt::_export::declare;

    use crate::pac::Interrupt as InterruptEnum;

    declare!(RTC);
    declare!(SPI0);
    declare!(SPI1);
    declare!(USART0);
    declare!(USART1);
    declare!(USART2);
    declare!(CAN0_RX1);
    declare!(DMA0_CHANNEL0);
    declare!(DMA0_CHANNEL1);
    declare!(DMA0_CHANNEL2);
    declare!(DMA0_CHANNEL3);
    declare!(DMA0_CHANNEL4);
    declare!(DMA0_CHANNEL5);
    declare!(DMA0_CHANNEL6);
    declare!(DMA1_CHANNEL0);
    declare!(DMA1_CHANNEL1);
    declare!(DMA1_CHANNEL2);
    declare!(DMA1_CHANNEL3_DMA1_CHANNEL4);
}
