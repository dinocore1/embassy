#![macro_use]

use core::ops::Deref;
use core::ptr;

use embassy_hal_common::{into_ref, PeripheralRef};
pub use embedded_hal_02::spi as hal;
use embedded_hal_02::spi::{Phase, Polarity};

use self::sealed::EnableGuard;
use crate::chip::peripherals;
use crate::interrupt::{Interrupt, InterruptExt};
use crate::pac::spi0::RegisterBlock as Regs;
use crate::{Hertz, Peripheral};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    BufLen,
    Overrun,
    DMAError(crate::dma::Error),
}

impl From<crate::dma::Error> for Error {
    fn from(err: crate::dma::Error) -> Error {
        Error::DMAError(err)
    }
}

pub struct Config {
    pub mode: hal::Mode,
    pub endian: Endian,
    pub target_baud: Hertz,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: hal::MODE_0,
            endian: Endian::MSB,
            target_baud: Hertz::mhz(1),
        }
    }
}

pub enum Endian {
    MSB,
    LSB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Prescaler {
    DIV2 = 0,
    DIV4 = 1,
    DIV8 = 2,
    DIV16 = 3,
    DIV32 = 4,
    DIV64 = 5,
    DIV128 = 6,
    DIV256 = 7,
}

impl From<Prescaler> for u8 {
    #[inline(always)]
    fn from(variant: Prescaler) -> Self {
        variant as _
    }
}

impl Prescaler {
    #[inline(always)]
    fn from_bits(bits: u8) -> Self {
        unsafe { core::mem::transmute(bits) }
        // match bits {
        //     0 => Prescaler::DIV2,
        //     1 => Prescaler::DIV4,
        //     2 => Prescaler::DIV8,
        //     3 => Prescaler::DIV16,
        //     4 => Prescaler::DIV32,
        //     5 => Prescaler::DIV64,
        //     6 => Prescaler::DIV128,
        //     7 => Prescaler::DIV256,
        //     _ => unreachable!(),
        // }
    }
}

impl crate::utils::ClockDivider for Prescaler {
    fn divide(&self, hz: Hertz) -> Hertz {
        match self {
            Prescaler::DIV2 => Hertz(hz.0 / 2),
            Prescaler::DIV4 => Hertz(hz.0 / 4),
            Prescaler::DIV8 => Hertz(hz.0 / 8),
            Prescaler::DIV16 => Hertz(hz.0 / 16),
            Prescaler::DIV32 => Hertz(hz.0 / 32),
            Prescaler::DIV64 => Hertz(hz.0 / 64),
            Prescaler::DIV128 => Hertz(hz.0 / 128),
            Prescaler::DIV256 => Hertz(hz.0 / 256),
        }
    }
}

fn compute_baud_rate(pclk: Hertz, target: Hertz) -> Prescaler {
    let val = match pclk.0 / target.0 {
        0 => unreachable!(),
        1..=2 => Prescaler::DIV2,
        3..=4 => Prescaler::DIV4,
        6..=8 => Prescaler::DIV8,
        7..=16 => Prescaler::DIV16,
        17..=32 => Prescaler::DIV32,
        33..=64 => Prescaler::DIV64,
        65..=128 => Prescaler::DIV128,
        129..=256 => Prescaler::DIV256,
        _ => unreachable!(),
    };
    val
}

fn check_error_flags(sr: &crate::pac::spi0::stat::R) -> Result<(), Error> {
    if sr.txurerr().bit_is_set() {
        return Err(Error::Overrun);
    }
    if sr.rxorerr().bit_is_set() {
        return Err(Error::Overrun);
    }
    Ok(())
}

fn spin_until_tx_ready(regs: &Regs) -> Result<(), Error> {
    loop {
        let sr = regs.stat.read();
        check_error_flags(&sr)?;
        if sr.tbe().bit_is_set() {
            return Ok(());
        }
    }
}

fn spin_until_rx_ready(regs: &Regs) -> Result<(), Error> {
    loop {
        let sr = regs.stat.read();
        check_error_flags(&sr)?;
        if sr.rbne().bit_is_set() {
            return Ok(());
        }
    }
}

fn transfer_word<W>(regs: &Regs, tx_word: W) -> Result<W, Error>
where
    W: Word,
{
    spin_until_tx_ready(regs)?;

    unsafe {
        ptr::write_volatile(regs.data.as_ptr() as *mut W, tx_word);
    }

    spin_until_rx_ready(regs)?;

    let rx_word = unsafe { ptr::read_volatile(regs.data.as_ptr() as *const W) };
    Ok(rx_word)
}

pub struct Spi<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
    current_word_size: crate::pac::spi0::ctl0::FF16_A,
}

impl<'d, T: Instance> Spi<'d, T> {
    pub fn new_master(
        spi: impl Peripheral<P = T> + 'd,
        sck: impl Peripheral<P = impl SckPin<T>> + 'd,
        mosi: impl Peripheral<P = impl MosiPin<T>> + 'd,
        miso: impl Peripheral<P = impl MisoPin<T>> + 'd,
        config: Config,
    ) -> Self {
        into_ref!(spi, sck, miso, mosi);

        // enable the clock to the SPI peripheral
        T::enable();

        let pclk = T::frequency();
        let prescaler = compute_baud_rate(pclk, config.target_baud);
        let baud_rate = pclk / prescaler;
        info!("SPI buad_rate: {}", baud_rate);

        let gpio_speed = crate::gpio::Speed::from(baud_rate);

        sck.set_as_output(crate::gpio::OutputType::AFPushPull, gpio_speed);
        miso.set_as_input(crate::gpio::Pull::None);
        mosi.set_as_output(crate::gpio::OutputType::AFPushPull, gpio_speed);

        let r = T::regs();
        r.ctl0.write(|w| {
            let w = match config.mode.polarity {
                Polarity::IdleLow => w.ckpl().clear_bit(),
                Polarity::IdleHigh => w.ckpl().set_bit(),
            };

            let w = match config.mode.phase {
                Phase::CaptureOnFirstTransition => w.ckph().clear_bit(),
                Phase::CaptureOnSecondTransition => w.ckph().set_bit(),
            };

            let w = match config.endian {
                Endian::MSB => w.lf().clear_bit(),
                Endian::LSB => w.lf().set_bit(),
            };

            let w = w.psc().bits(u8::from(prescaler));

            // config for master mode full-duplex
            let w = w.mstmod().set_bit();
            let w = w.ro().clear_bit();
            let w = w.bden().clear_bit();

            w
        });

        Self {
            _p: spi,
            current_word_size: crate::pac::spi0::ctl0::FF16_A::EIGHT_BIT,
        }
    }

    pub fn new_slave(
        spi: impl Peripheral<P = T> + 'd,
        sck: impl Peripheral<P = impl SckPin<T>> + 'd,
        mosi: impl Peripheral<P = impl MosiPin<T>> + 'd,
        miso: impl Peripheral<P = impl MisoPin<T>> + 'd,
        config: Config,
    ) -> Self {
        into_ref!(spi, sck, miso, mosi);

        // enable the clock to the SPI peripheral
        T::enable();

        let gpio_speed = crate::gpio::Speed::from(config.target_baud);

        sck.set_as_input(crate::gpio::Pull::None);
        miso.set_as_output(crate::gpio::OutputType::AFPushPull, gpio_speed);
        mosi.set_as_input(crate::gpio::Pull::None);

        let r = T::regs();
        r.ctl0.write(|w| {
            let w = match config.mode.polarity {
                Polarity::IdleLow => w.ckpl().clear_bit(),
                Polarity::IdleHigh => w.ckpl().set_bit(),
            };

            let w = match config.mode.phase {
                Phase::CaptureOnFirstTransition => w.ckph().clear_bit(),
                Phase::CaptureOnSecondTransition => w.ckph().set_bit(),
            };

            let w = match config.endian {
                Endian::MSB => w.lf().clear_bit(),
                Endian::LSB => w.lf().set_bit(),
            };

            // config for save mode full-duplex
            let w = w.mstmod().clear_bit();
            let w = w.ro().clear_bit();
            let w = w.bden().clear_bit();

            w
        });

        Self {
            _p: spi,
            current_word_size: crate::pac::spi0::ctl0::FF16_A::EIGHT_BIT,
        }
    }

    // fn on_interrupt(_: *mut()) {
    //     let r = T::regs();
    //     let s = T::state();
    // }

    // irq.set_handler(Self::on_interrupt);
    // irq.unpend();
    // irq.enable();

    fn set_word_size(&mut self, word_size: crate::pac::spi0::ctl0::FF16_A) {
        if self.current_word_size == word_size {
            return;
        }

        let r = T::regs();
        r.ctl0.modify(|_, w| w.ff16().variant(word_size));
        self.current_word_size = word_size;
    }

    pub async fn read<'a, W, Tx, Rx>(
        &mut self,
        tx_dma: PeripheralRef<'a, Tx>,
        rx_dma: PeripheralRef<'a, Rx>,
        rx: &mut [W],
    ) -> Result<(), Error>
    where
        W: Word,
        Tx: TxDma<T>,
        Rx: RxDma<T>,
    {
        let regs = T::regs();
        let count: u16 = rx.len().try_into().map_err(|_| Error::BufLen)?;

        // configure DMA transfers
        let dma_write = crate::dma::write_repeated(tx_dma, W::default(), regs.data.as_ptr(), count);
        let dma_read = crate::dma::read(rx_dma, regs.data.as_ptr(), rx.as_mut_ptr(), count);

        // enable DMA transfer mode
        regs.ctl1.modify(|_, w| w.dmaten().set_bit().dmaren().set_bit());

        let _enable_guard = EnableGuard::new(regs);

        futures::try_join!(dma_write, dma_read)?;
        Ok(())
    }

    pub async fn write<'a, W, Tx, Rx>(
        &mut self,
        tx_dma: PeripheralRef<'a, Tx>,
        rx_dma: PeripheralRef<'a, Rx>,
        tx: &[W],
    ) -> Result<(), Error>
    where
        W: Word,
        Tx: TxDma<T>,
        Rx: RxDma<T>,
    {
        let regs = T::regs();
        let count: u16 = tx.len().try_into().map_err(|_| Error::BufLen)?;

        let mut rx = [0_u8];

        // configure DMA transfers
        let dma_write = crate::dma::write(tx_dma, tx.as_ptr(), regs.data.as_ptr(), count);
        let dma_read = crate::dma::read_repeated(rx_dma, regs.data.as_ptr(), rx.as_mut_ptr(), count);

        // enable DMA transfer mode
        regs.ctl1.modify(|_, w| w.dmaten().set_bit().dmaren().set_bit());

        let _enable_guard = EnableGuard::new(regs);

        futures::try_join!(dma_write, dma_read)?;
        Ok(())
    }

    pub async fn transfer<'a, W, Tx, Rx>(
        &mut self,
        tx_dma: PeripheralRef<'a, Tx>,
        rx_dma: PeripheralRef<'a, Rx>,
        tx: &[W],
        rx: &mut [W],
    ) -> Result<(), Error>
    where
        W: Word,
        Tx: TxDma<T>,
        Rx: RxDma<T>,
    {
        let regs = T::regs();

        let count = tx.len();
        if count != rx.len() {
            return Err(Error::BufLen);
        }
        let count: u16 = count.try_into().map_err(|_| Error::BufLen)?;

        // configure DMA transfers
        let dma_write = crate::dma::write(tx_dma, tx.as_ptr(), regs.data.as_ptr(), count);
        let dma_read = crate::dma::read(rx_dma, regs.data.as_ptr(), rx.as_mut_ptr(), count);

        // enable DMA transfer mode
        regs.ctl1.modify(|_, w| w.dmaten().set_bit().dmaren().set_bit());

        let _enable_guard = EnableGuard::new(regs);

        futures::try_join!(dma_write, dma_read)?;
        Ok(())
    }

    pub fn blocking_transfer_in_place<W>(&mut self, buf: &mut [W]) -> Result<(), Error>
    where
        W: Word,
    {
        let regs = T::regs();

        self.set_word_size(W::FF16);

        let _enable_guard = EnableGuard::new(regs);

        for word in buf.iter_mut() {
            *word = transfer_word(regs, *word)?;
        }

        Ok(())
    }

    pub fn blocking_transfer<W>(&mut self, tx: &[W], rx: &mut [W]) -> Result<(), Error>
    where
        W: Word,
    {
        let regs = T::regs();

        self.set_word_size(W::FF16);

        let _enable_guard = EnableGuard::new(regs);

        let len = tx.len().max(rx.len());
        for i in 0..len {
            let wb = rx.get(i).copied().unwrap_or_default();
            let rb = transfer_word(regs, wb)?;
            if let Some(r) = rx.get_mut(i) {
                *r = rb;
            }
        }

        Ok(())
    }
}

pub(crate) mod sealed {
    use embassy_sync::waitqueue::AtomicWaker;

    pub struct State {
        pub end_waker: AtomicWaker,
    }

    impl State {
        pub const fn new() -> Self {
            Self {
                end_waker: AtomicWaker::new(),
            }
        }
    }

    pub trait Instance {
        fn regs() -> &'static crate::pac::spi0::RegisterBlock;
        fn state() -> &'static State;
    }

    pub trait Word: Copy + 'static {
        const WORDSIZE: WordSize;
        const FF16: crate::pac::spi0::ctl0::FF16_A;
    }

    impl Word for u8 {
        const WORDSIZE: WordSize = WordSize::Bit8;
        const FF16: crate::pac::spi0::ctl0::FF16_A = crate::pac::spi0::ctl0::FF16_A::EIGHT_BIT;
    }

    impl Word for u16 {
        const WORDSIZE: WordSize = WordSize::Bit16;
        const FF16: crate::pac::spi0::ctl0::FF16_A = crate::pac::spi0::ctl0::FF16_A::SIXTEEN_BIT;
    }

    #[derive(Clone, Copy, PartialEq, PartialOrd)]
    pub enum WordSize {
        Bit8,
        Bit16,
    }

    impl WordSize {
        pub fn ff16(&self) -> crate::pac::spi0::ctl0::FF16_A {
            match self {
                WordSize::Bit8 => crate::pac::spi0::ctl0::FF16_A::EIGHT_BIT,
                WordSize::Bit16 => crate::pac::spi0::ctl0::FF16_A::SIXTEEN_BIT,
            }
        }
    }

    pub struct EnableGuard<'a> {
        regs: &'a crate::pac::spi0::RegisterBlock,
    }

    impl<'a> EnableGuard<'a> {
        pub fn new(regs: &'a crate::pac::spi0::RegisterBlock) -> Self {
            let guard = Self { regs };
            guard.enable();
            guard
        }

        pub fn enable(&self) {
            // enable SPI
            self.regs.ctl0.modify(|_, w| w.spien().set_bit());
        }

        pub fn disable(&self) {
            // disable SPI
            self.regs.ctl0.modify(|_, w| w.spien().clear_bit());
        }
    }

    impl<'a> Drop for EnableGuard<'a> {
        fn drop(&mut self) {
            self.disable();
        }
    }
}

pub trait Word: Copy + 'static + sealed::Word + crate::dma::Word + Default {}
impl Word for u8 {}
impl Word for u16 {}

pin_trait!(SckPin, Instance);
pin_trait!(MosiPin, Instance);
pin_trait!(MisoPin, Instance);

dma_trait!(TxDma, Instance);
dma_trait!(RxDma, Instance);

pub trait Instance: Peripheral<P = Self> + sealed::Instance + crate::cctl::CCTLPeripherial + 'static {
    type Interrupt: Interrupt;
}

macro_rules! impl_spi {
    ($type:ident, $pac_type:ident, $irq:ident) => {
        impl crate::spi::sealed::Instance for peripherals::$type {
            fn regs() -> &'static crate::pac::spi0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::spi0::RegisterBlock) }
            }

            fn state() -> &'static crate::spi::sealed::State {
                static STATE: crate::spi::sealed::State = crate::spi::sealed::State::new();
                &STATE
            }
        }

        impl crate::spi::Instance for peripherals::$type {
            type Interrupt = crate::interrupt::$irq;
        }
    };
}

impl crate::cctl::CCTLPeripherial for peripherals::SPI0 {
    fn frequency() -> crate::utils::Hertz {
        let r = unsafe { &*crate::pac::SPI0::ptr() };
        let prescaler = crate::spi::Prescaler::from_bits(r.ctl0.read().psc().bits());
        let clocks = crate::cctl::get_freq();
        clocks.apb2 / prescaler
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.spi0en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.spi0en().clear_bit())
    }
}

impl crate::cctl::CCTLPeripherial for peripherals::SPI1 {
    fn frequency() -> crate::utils::Hertz {
        let r = unsafe { &*crate::pac::SPI1::ptr() };
        let prescaler = crate::spi::Prescaler::from_bits(r.ctl0.read().psc().bits());
        let clocks = crate::cctl::get_freq();
        clocks.apb1 / prescaler
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.spi1en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.spi1en().clear_bit())
    }
}
