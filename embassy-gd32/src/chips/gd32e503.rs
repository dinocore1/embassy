
pub(crate) use gd32e5::gd32e503 as pac;

pub const FLASH_SIZE: usize = 512 * 1024;

embassy_hal_common::peripherals! {
    PMU,
    RTC,
    RCU,
    DMA0,
    DMA1,
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
}