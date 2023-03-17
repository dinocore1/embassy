#![macro_use]

use core::task::{Poll, Context};

use embassy_cortex_m::interrupt::Priority;
use embassy_hal_common::{into_ref, PeripheralRef, Peripheral};
//use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_sync::channel::{Channel};
use crate::chip::peripherals;
use crate::interrupt::{Interrupt, InterruptExt};
use crate::utils::{Hertz};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {

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
    rx_channel: Channel<NoopRawMutex, u8, 4>,
    tx_channel: Channel<NoopRawMutex, u8, 4>,
}

impl<'d, T> Uart<'d, T>
where T: Instance
{

    pub fn new(uart: impl Peripheral<P = T> + 'd, 
        tx: impl Peripheral<P = impl TxPin<T>> + 'd, 
        rx: impl Peripheral<P = impl RxPin<T>> + 'd,
        config: Config,
    ) -> Self
    {
        into_ref!(uart, tx, rx);

        T::enable();
        
        tx.set_as_output(crate::gpio::OutputType::AFPushPull, crate::gpio::Speed::Low);
        rx.set_as_input(crate::gpio::Pull::Up);

        let mut this = Self { _p: uart, rx_channel: Channel::new(), tx_channel: Channel::new() };
        this.config(config);
        this
    }

    pub fn config(&mut self, config: Config) {
        let regs = T::regs();

        let (parity_enable, parity_mode) = match config.parity {
            Parity::ParityNone => (false, false),
            Parity::ParityOdd => (true, true),
            Parity::ParityEven => (true, false),
        };

        let stop_bits_value = match config.stop_bits {
            StopBits::STOP1 => 0b00_u8,
            StopBits::STOP2 => 0b10_u8,
        };

        let pclk = T::frequency();
        let bauddiv = calc_bauddiv(pclk, config.baudrate, Oversample::SixteenTimes);

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

    pub fn blocking_write(&mut self, buf: &[u8]) -> Result<(), Error> {
        let regs = T::regs();

        for byte in buf {
            while regs.stat0.read().tbe().bit_is_clear() {}
            regs.data.write(|w| w.data().variant( *byte as u16 ));
        }

        while regs.stat0.read().tc().bit_is_clear() {}
        Ok(())
    }

    pub fn blocking_read(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        let regs = T::regs();

        for i in 0..buf.len() {
            while regs.stat0.read().rbne().bit_is_clear() {}
            buf[i] = regs.data.read().data().bits() as u8;
        }
        Ok(())
    }

    fn wait_for_tbe(cx: &mut Context) -> Poll<()> {
        let regs = T::regs();
        if regs.stat0.read().tbe().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    fn wait_for_tc(cx: &mut Context) -> Poll<()> {
        let regs = T::regs();
        if regs.stat0.read().tc().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    fn wait_for_rbne(cx: &mut Context) -> Poll<()> {
        let regs = T::regs();
        if regs.stat0.read().rbne().bit_is_set() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    pub async fn write(&mut self, buf: &[u8]) -> Result<(), Error> {
        let regs = T::regs();

        for byte in buf {
            core::future::poll_fn(Self::wait_for_tbe).await;
            regs.data.write(|w| w.data().variant( *byte as u16 ));
        }
        core::future::poll_fn(Self::wait_for_tc).await;
        Ok(())
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        let regs = T::regs();

        for i in 0..buf.len() {
            core::future::poll_fn(Self::wait_for_rbne).await;
            buf[i] = regs.data.read().data().bits() as u8;
        }
        Ok(())
    }

    fn setup_interrupt_handler<'a>(&'a mut self, interrupt: PeripheralRef<'a, T::Interrupt>) {

        interrupt.set_handler_context(self as *mut _ as *mut ());
        interrupt.set_handler(unsafe { core::mem::transmute(Self::on_interrupt as *const()) });

        interrupt.set_priority(Priority::P2);
        interrupt.unpend();
        interrupt.enable();
    }

    pub fn receiver(&mut self, interrupt: PeripheralRef<'_, T::Interrupt>) -> Receiver<NoopRawMutex, u8, 4> {
        let regs = T::regs();
        regs.ctl0.modify(|_, w| w.rbneie().set_bit());
        self.setup_interrupt_handler(interrupt);
        self.rx_channel.receiver()
    }

    fn on_interrupt(&mut self) {
        let regs = T::regs();

        let stat0 = regs.stat0.read();

        if stat0.rbne().bit_is_set() {
            let data = regs.data.read().data().bits() as u8;
            if let Err(_) = self.rx_channel.try_send(data) {
                error!("rx_channel full");
            }
        }

        if stat0.tbe().bit_is_set() {
            if let Ok(data) = self.rx_channel.try_recv() {
                regs.data.write(|w| w.data().variant(data as u16));
            }
        }
    }


}

impl<'d, T> core::fmt::Write for Uart<'d, T>
where T: Instance
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.blocking_write(s.as_bytes()).map_err(|_| core::fmt::Error)
    }
}


pub(crate) mod sealed {
    use super::*;

    pub trait Instance {
        fn regs() -> &'static crate::pac::usart0::RegisterBlock;
    }
}

pin_trait!(TxPin, Instance);
pin_trait!(RxPin, Instance);

pub trait Instance: Peripheral<P = Self> + sealed::Instance + crate::cctl::CCTLPeripherial {
    type Interrupt: Interrupt;
}

macro_rules! impl_usart {
    ($type:ident, $pac_type:ident, $irq:ident, $waker:ident) => {

        impl crate::usart::sealed::Instance for peripherals::$type {
            fn regs() -> &'static crate::pac::usart0::RegisterBlock {
                unsafe { &*(crate::pac::$pac_type::ptr() as *const crate::pac::usart0::RegisterBlock) }
            }
        }

        impl crate::usart::Instance for peripherals::$type {
            type Interrupt = crate::interrupt::$irq;
        }
        
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum Oversample {
    EightTimes,
    SixteenTimes,
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