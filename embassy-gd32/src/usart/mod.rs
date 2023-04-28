#![macro_use]
use core::task::{Poll, Context};

use crate::{chip::peripherals, Hertz};
use embassy_cortex_m::peripheral::{PeripheralMutex, PeripheralState, StateStorage};
use embassy_hal_common::{into_ref, PeripheralRef, Peripheral};
use embassy_cortex_m::interrupt::InterruptExt;
use embassy_sync::waitqueue::WakerRegistration;
use futures::Future;

use crate::pac::usart0::RegisterBlock as Regs;

mod buffered;
pub use buffered::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    Overrun,
}

#[cfg(feature = "nightly")]
impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataBits {
    DataBits5,
    DataBits6,
    DataBits7,
    DataBits8,
}

impl DataBits {
    fn bits(&self) -> u8 {
        match self {
            Self::DataBits5 => 0b00,
            Self::DataBits6 => 0b01,
            Self::DataBits7 => 0b10,
            Self::DataBits8 => 0b11,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Parity {
    ParityNone,
    ParityEven,
    ParityOdd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StopBits {
    #[doc = "1 stop bit"]
    STOP1,
    #[doc = "2 stop bits"]
    STOP2,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum Oversample {
    EightTimes,
    SixteenTimes,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Config {
    pub baudrate: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            baudrate: 115200,
            data_bits: DataBits::DataBits8,
            stop_bits: StopBits::STOP1,
            parity: Parity::ParityNone,
        }
    }
}

pub struct Uart<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
}

impl<'d, T: Instance> Uart<'d, T> {

    pub fn new(
        p: impl Peripheral<P = T> + 'd,
        tx_pin: impl Peripheral<P = impl TxPin<T>> + 'd,
        rx_pin: impl Peripheral<P = impl RxPin<T>> + 'd,
        config: Config,
    ) -> Self {
        into_ref!(p, tx_pin, rx_pin);

        T::enable();

        tx_pin.set_as_output(crate::gpio::OutputType::AFPushPull, crate::gpio::Speed::Low);
        rx_pin.set_as_input(crate::gpio::Pull::Up);

        let regs = T::regs();
        let pclk_freq = T::frequency();
        configure(regs, &config, pclk_freq);


        Self {
            _p: p,
        }
    }

    pub fn blocking_write(&mut self, buf: &[u8]) -> Result<(), Error> {
        blocking_write(T::regs(), buf)
    }

    pub fn blocking_read(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        blocking_read(T::regs(), buf)
    }

    pub fn polling_write<'a>(&mut self, buf: &'a [u8]) -> impl Future<Output=Result<(), Error>> + 'a {
        polling_write(T::regs(), &buf)
    }

    pub fn polling_read<'a>(&mut self, buf: &'a mut [u8]) -> impl Future<Output=Result<(), Error>> + 'a {
        polling_read(T::regs(), buf)
    }

}

fn configure(regs: &Regs, config: &Config, pclk_freq: Hertz) {
    let (parity_enable, parity_mode) = match config.parity {
        Parity::ParityNone => (false, false),
        Parity::ParityOdd => (true, true),
        Parity::ParityEven => (true, false),
    };

    let stop_bits_value = match config.stop_bits {
        StopBits::STOP1 => 0b00_u8,
        StopBits::STOP2 => 0b10_u8,
    };

    let bauddiv = calc_bauddiv(pclk_freq, config.baudrate, Oversample::SixteenTimes);

    regs.baud.write(|w| unsafe { w.bits(bauddiv as u32) });
    regs.ctl1.write(|w| w.stb().variant(stop_bits_value) );
    regs.ctl2.reset();

    regs.ctl0.write(|w| w
        .uen().set_bit()
        .pcen().variant(parity_enable)
        .pm().variant(parity_mode)
        .ten().set_bit()
        .ren().set_bit()
    );
}

fn calc_bauddiv(pclk: Hertz, baud: u32, oversample: Oversample) -> u16 {

    let div = match oversample {
        Oversample::SixteenTimes => {
            (pclk.0 + baud/2) / baud
        }

        Oversample::EightTimes => {
            ((pclk.0 + baud/2) << 1) / baud
        }
    };

    // let intdiv = div & 0xfff0;
    // let fradiv = div & 0xf;

    // (intdiv as u16) | (fradiv as u16)
    div as u16
}

fn blocking_write(regs: &Regs, buf: &[u8]) -> Result<(), Error> {
    for byte in buf {
        while regs.stat0.read().tbe().bit_is_clear() {}
        regs.data.write(|w| w.data().variant( *byte as u16 ));
    }

    while regs.stat0.read().tc().bit_is_clear() {}
    Ok(())
}

fn blocking_read(regs: &Regs, buf: &mut [u8]) -> Result<(), Error> {
    for i in 0..buf.len() {
        while regs.stat0.read().rbne().bit_is_clear() {}
        buf[i] = regs.data.read().data().bits() as u8;
    }
    Ok(())
}

async fn polling_write(regs: &Regs, buf: &[u8]) -> Result<(), Error> {
    for byte in buf {
        WaitForTBE { regs }.await;
        regs.data.write(|w| w.data().variant( *byte as u16 ));
    }
    WaitForTC { regs }.await;
    Ok(())
}

async fn polling_read(regs: &Regs, buf: &mut [u8]) -> Result<(), Error> {
    for i in 0..buf.len() {
        WaitForRBNE { regs }.await;
        buf[i] = regs.data.read().data().bits() as u8;
    }
    Ok(())
}

struct WaitForTBE<'a> {
    regs: &'a Regs,
}

impl<'a> Future for WaitForTBE<'a> {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.regs.stat0.read().tbe().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct WaitForTC<'a> {
    regs: &'a Regs,
}

impl<'a> Future for WaitForTC<'a> {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.regs.stat0.read().tc().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct WaitForRBNE<'a> {
    regs: &'a Regs,
}

impl<'a> Future for WaitForRBNE<'a> {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.regs.stat0.read().rbne().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub(crate) mod sealed {
    use embassy_sync::waitqueue::AtomicWaker;

    use super::*;

    pub struct State {
        pub rx_waker: AtomicWaker,
        pub tx_waker: AtomicWaker,
    }

    impl State {
        pub const fn new() -> Self {
            Self { 
                rx_waker: AtomicWaker::new(),
                tx_waker: AtomicWaker::new(),
            }
        }
    }

    pub trait Instance: crate::cctl::CCTLPeripherial {
        type Interrupt: crate::interrupt::Interrupt;

        fn regs() -> &'static Regs;
        fn state() -> &'static State;
    }
}

pub trait Instance: Peripheral<P = Self> + sealed::Instance + 'static + Send {}

pin_trait!(TxPin, Instance);
pin_trait!(RxPin, Instance);


macro_rules! impl_usart {
    ($type:ident, $pac_type:ident, $irq:ident) => {

        impl crate::usart::sealed::Instance for peripherals::$type {
            type Interrupt = crate::interrupt::$irq;
            fn regs() -> &'static crate::pac::usart0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::usart0::RegisterBlock) }
            }

            fn state() -> &'static crate::usart::sealed::State {
                static STATE: crate::usart::sealed::State = crate::usart::sealed::State::new();
                &STATE
            }
        }

        impl crate::usart::Instance for peripherals::$type {}
        
    };
}

/// USART0 uses PCLK2
impl crate::cctl::CCTLPeripherial for peripherals::USART0 {
    fn frequency() -> crate::utils::Hertz {
        let clocks = crate::cctl::get_freq();
        clocks.apb2
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.usart0en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb2en.modify(|_, w| w.usart0en().clear_bit())
    }
}

/// USART1 uses PCLK1
impl crate::cctl::CCTLPeripherial for peripherals::USART1 {
    fn frequency() -> crate::utils::Hertz {
        let clocks = crate::cctl::get_freq();
        clocks.apb1
    }

    fn enable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.usart1en().set_bit())
    }

    fn disable() {
        let rcu = unsafe { crate::chip::pac::Peripherals::steal().RCU };
        rcu.apb1en.modify(|_, w| w.usart1en().clear_bit())
    }
}

impl<'d, T> core::fmt::Write for Uart<'d, T>
where T: Instance
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.blocking_write(s.as_bytes()).map_err(|_| core::fmt::Error)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_calc_bauddiv_16() {
        let pclk = Hertz::mhz(32);
        let baud = 115200;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::SixteenTimes);

        //17.36
        let intdiv = bauddiv >> 4;
        assert_eq!(17, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(6, fradiv);
    }

    #[test]
    fn test_calc_bauddiv_8() {
        let pclk = Hertz::mhz(32);
        let baud = 115200;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::EightTimes);

        //34.72
        let intdiv = bauddiv >> 4;
        assert_eq!(34, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(12, fradiv);
    }

    #[test]
    fn test2_calc_bauddiv_16() {
        let pclk = Hertz::mhz(32);
        let baud = 900;
        let bauddiv = calc_bauddiv(pclk, baud, Oversample::SixteenTimes);

        //2222.25
        let intdiv = bauddiv >> 4;
        assert_eq!(2222, intdiv);

        let fradiv = bauddiv & 0xf;
        assert_eq!(4, fradiv);
    }
}