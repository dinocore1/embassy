

use core::{cell::UnsafeCell, future::{poll_fn, Pending}};

use super::*;


pub struct State<'d, T: Instance>(StateStorage<StateInner<'d, T>>);
impl<'d, T: Instance> State<'d, T> {
    pub const fn new() -> Self {
        Self(StateStorage::new())
    }
}

pub struct UartBuffered<'d, T: Instance> {
    inner: UnsafeCell<PeripheralMutex<'d, StateInner<'d, T>>>,
}

impl<'d, T: Instance> UartBuffered<'d, T> {

    pub fn new(
        state: &'d mut State<'d, T>,
        p: impl Peripheral<P = T> + 'd,
        tx_pin: impl Peripheral<P = impl TxPin<T>> + 'd,
        rx_pin: impl Peripheral<P = impl RxPin<T>> + 'd,
        irq: impl Peripheral<P = T::Interrupt> + 'd,
        tx_buffer: &'d mut [u8],
        rx_buffer: &'d mut [u8],
        config: Config,
    ) -> Self {
        into_ref!(p, tx_pin, rx_pin, irq);
        T::enable();

        tx_pin.set_as_output(crate::gpio::OutputType::AFPushPull, crate::gpio::Speed::Low);
        rx_pin.set_as_input(crate::gpio::Pull::Up);

        let regs = T::regs();
        let pclk_freq = T::frequency();
        configure(regs, &config, pclk_freq);

        Self {
            inner: UnsafeCell::new(PeripheralMutex::new(irq, &mut state.0, move || StateInner {
                _p: p,
                tx: RingBuffer::new(tx_buffer),
                tx_waker: WakerRegistration::new(),
                rx: RingBuffer::new(rx_buffer),
                rx_waker: WakerRegistration::new(),
            }))
        }
    }

    pub async fn inner_read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        poll_fn(move |cx| {
            let inner = unsafe { &mut *self.inner.get() };
            inner.with(|state| {
                if !state.rx.is_empty() {
                    let data = state.rx.pop_buf();
                    let len = data.len().min(buf.len());
                    buf[..len].copy_from_slice(&data[..len]);
                    state.rx.pop(len);
                    Poll::Ready(Ok(len))
                } else {
                    T::regs().ctl0.modify(|_, w| w.rbneie().set_bit());
                    state.rx_waker.register(cx.waker());
                    Poll::Pending
                }
            })
        }).await
    }

    pub async fn inner_write(&self, buf: &[u8]) -> Result<usize, Error> {
        poll_fn(move |cx| {
            let inner = unsafe { &mut *self.inner.get() };
            inner.with(|state| {
                if !state.tx.is_full() {
                    let tx_buf = state.tx.push_buf();
                    let len = tx_buf.len().min(buf.len());
                    tx_buf[..len].copy_from_slice(&buf[..len]);
                    state.tx.push(len);
                    T::regs().ctl0.modify(|_, w| w.tbeie().set_bit());
                    Poll::Ready(Ok(len))
                } else {
                    state.tx_waker.register(cx.waker());
                    Poll::Pending
                }
            })
        }).await
    }

    async fn inner_flush(&self) -> Result<(), Error> {
        poll_fn(move |cx| {
            let inner = unsafe { &mut *self.inner.get() };
            inner.with(|state| {
                if !state.tx.is_empty() {
                    state.tx_waker.register(cx.waker());
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(()))
                }
            })
        }).await
    }

    fn inner_blocking_flush(&self) -> Result<(), Error> {
        let inner = unsafe { &mut *self.inner.get() };
        inner.with(|state| {
            let regs = T::regs();
            let tx_buf = state.tx.pop_buf();
            let len = tx_buf.len();
            super::blocking_write(regs, tx_buf)?;
            state.tx.pop(len);
            Ok(())
        })
    }

    pub fn inner_blocking_write(&self, buf: &[u8]) -> Result<usize, Error> {
        let inner = unsafe { &mut *self.inner.get() };
        inner.with(|state| {
            let regs = T::regs();
            {
                // flush anything in the tx buffer
                let tx_buf = state.tx.pop_buf();
                let len = tx_buf.len();
                super::blocking_write(regs, tx_buf)?;
                state.tx.pop(len);
            }

            super::blocking_write(regs, buf)
        }).map(|_| {
            buf.len()
        })
        
    }

    pub fn split(&mut self) -> (BufferedUartRx<'_, 'd, T>, BufferedUartTx<'_, 'd, T>) {
        (BufferedUartRx { inner: self }, BufferedUartTx { inner: self })
    }

}

// impl<'d, T: Instance> core::fmt::Write for UartBuffered<'d, T>
// {
//     fn write_str(&mut self, s: &str) -> core::fmt::Result {
//         self.write_all()
//         self.inner_blocking_write(s.as_bytes).map_err(|_| core::fmt::Error)
//     }
// }

impl<'d, T: Instance> UartBuffered<'d, T> {

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner_read(buf).await
    }

    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.inner_write(buf).await
    }

    pub async fn flush(&mut self) -> Result<(), Error> {
        self.inner_flush().await
    }
}

pub struct BufferedUartRx<'d, 'a, T: Instance> {
    inner: &'d UartBuffered<'a, T>,
}

impl<'d, 'a, T: Instance> BufferedUartRx<'d, 'a, T> {
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.inner_read(buf).await
    }
}

impl<'d, 'a, T: Instance> BufferedUartTx<'d, 'a, T> {
    pub async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.inner.inner_write(buf).await
    }
}

#[cfg(feature = "nightly")]
impl<'d, T: Instance> embedded_io::asynch::Read for UartBuffered<'d, T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner_read(buf).await
    }
}

#[cfg(feature = "nightly")]
impl<'d, T: Instance> embedded_io::asynch::Write for UartBuffered<'d, T> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.inner_write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Error> {
        self.inner_flush().await
    }
}

#[cfg(feature = "nightly")]
impl<'d, 'a, T: Instance> embedded_io::asynch::Read for BufferedUartRx<'d, 'a, T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.inner.inner_read(buf).await
    }
}

#[cfg(feature = "nightly")]
impl<'d, 'a, T: Instance> embedded_io::asynch::Write for BufferedUartTx<'d, 'a, T> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.inner.inner_write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Error> {
        self.inner.inner_flush().await
    }
}

#[cfg(feature = "nightly")]
impl<'d, 'a, T: Instance> embedded_io::blocking::Write for BufferedUartTx<'d, 'a, T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.inner.inner_blocking_write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.inner_blocking_flush()
    }
}

#[cfg(feature = "nightly")]
impl<'d, T: Instance> embedded_io::Io for UartBuffered<'d, T> {
    type Error = super::Error;
}

#[cfg(feature = "nightly")]
impl<'d, 'a, T: Instance> embedded_io::Io for BufferedUartRx<'d, 'a, T> {
    type Error = super::Error;
}

#[cfg(feature = "nightly")]
impl<'d, 'a, T: Instance> embedded_io::Io for BufferedUartTx<'d, 'a, T> {
    type Error = super::Error;
}



pub struct BufferedUartTx<'d, 'a, T: Instance> {
    inner: &'d UartBuffered<'a, T>,
}



struct StateInner<'d, T: Instance> {
    _p: PeripheralRef<'d, T>,
    rx: RingBuffer<'d>,
    rx_waker: WakerRegistration,
    tx: RingBuffer<'d>,
    tx_waker: WakerRegistration,
}

impl<'a, T: Instance> PeripheralState for StateInner<'a, T> {
    type Interrupt = T::Interrupt;

    fn on_interrupt(&mut self) {

        let regs = T::regs();
        let stat0 = regs.stat0.read();

        if stat0.orerr().bit_is_set() {
            warn!("Overrun error");
        }

        if stat0.nerr().bit_is_set() {
            warn!("Noise error");
        }

        if stat0.ferr().bit_is_set() {
            warn!("Frame error");
        }

        if stat0.perr().bit_is_set() {
            warn!("Parity error");
        }

        if stat0.rbne().bit_is_set() {
            let byte = regs.data.read().data().bits() as u8;
            let buf = self.rx.push_buf();
            if !buf.is_empty() {
                buf[0] = byte;
                self.rx.push(1);
            } else {
                warn!("RX buffer full");
            }

            self.rx_waker.wake();
        }

        if stat0.tbe().bit_is_set() {
            let buf = self.tx.pop_buf();
            if !buf.is_empty() {
                regs.data.write(|w| w.data().variant(buf[0].into()));
                self.tx.pop(1);
                self.tx_waker.wake();
            } else {
                // Disable the interrupt until we have something to transmit again
                regs.ctl0.modify(|_, w| w.tbeie().clear_bit());
            }
        }
        
    }
}
