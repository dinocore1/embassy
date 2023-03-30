

use core::{
    cell::UnsafeCell,
    future::{poll_fn, Pending}
};
use embassy_hal_common::atomic_ring_buffer;

use super::*;


pub struct State<'d, T: Instance> {
    irq_state: StateStorage<StateInner<'d, T>>,
    tx: atomic_ring_buffer::RingBuffer,
    rx: atomic_ring_buffer::RingBuffer,
}

impl<'d, T: Instance> State<'d, T> {
    pub const fn new() -> Self {
        Self {
            irq_state: StateStorage::new(),
            tx: atomic_ring_buffer::RingBuffer::new(),
            rx: atomic_ring_buffer::RingBuffer::new(),
        }
    }
}

pub struct UartBuffered<'d, T: Instance> {
    irq_state: UnsafeCell<PeripheralMutex<'d, StateInner<'d, T>>>,
    rx: &'d atomic_ring_buffer::RingBuffer,
    tx: &'d atomic_ring_buffer::RingBuffer,
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

        
        unsafe {
            state.tx.init(tx_buffer.as_mut_ptr(), tx_buffer.len());
            state.rx.init(rx_buffer.as_mut_ptr(), rx_buffer.len());
        }

        let regs = T::regs();
        regs.ctl0.modify(|_, w| w.rbneie().set_bit());

        let rx_writer = unsafe { state.rx.writer() };
        let tx_reader = unsafe { state.tx.reader() };

        let irq_state = PeripheralMutex::new(irq, &mut state.irq_state, move || StateInner {
            _p: p,
            rx_waker: WakerRegistration::new(),
            rx_writer,
            tx_waker: WakerRegistration::new(),
            tx_reader,
        });

        Self {
            irq_state: UnsafeCell::new(irq_state),
            tx: &state.tx,
            rx: &state.rx
        }
    }

    pub async fn inner_read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        poll_fn(move |cx| {
            
            let mut reader = unsafe { self.rx.reader() };

            let inner = unsafe { &mut *self.irq_state.get() };
            let (data_ptr, n) = inner.with(|state| {
                let (data_ptr, n) = reader.pop_buf();
                if n == 0 {
                    state.rx_waker.register(cx.waker());
                }
                (data_ptr, n)
            });

            if n > 0 {
                let len = n.min(buf.len());
                let data = unsafe { core::slice::from_raw_parts(data_ptr, len) };
                buf[..len].copy_from_slice(data);
                reader.pop_done(len);
                Poll::Ready(Ok(len))
            } else {
                Poll::Pending
            }
        }).await
    }

    pub async fn inner_write(&self, buf: &[u8]) -> Result<usize, Error> {
        poll_fn(move |cx| {

            let mut writer = unsafe { self.tx.writer() };

            let inner = unsafe { &mut *self.irq_state.get() };
            let (data_ptr, n) = inner.with(|state| {
                let (data_ptr, n) = writer.push_buf();
                if n == 0 {
                    // if the TX buffer is full, we have to wait
                    state.tx_waker.register(cx.waker());
                }
                (data_ptr, n)
            });

            if n > 0 {
                let len = n.min(buf.len());
                let data = unsafe { core::slice::from_raw_parts_mut(data_ptr, len) };
                data[..len].copy_from_slice(&buf[..len]);
                writer.push_done(len);
                core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
                T::regs().ctl0.modify(|_, w| w.tbeie().set_bit());
                Poll::Ready(Ok(len))
            } else {
                Poll::Pending
            }
        }).await
    }

    async fn inner_flush(&self) -> Result<(), Error> {
        poll_fn(move |cx| {
            
            let inner = unsafe { &mut *self.irq_state.get() };
            inner.with(|state| {
                if !self.tx.is_empty() {
                    //T::regs().ctl0.modify(|_, w| w.tbeie().set_bit());
                    state.tx_waker.register(cx.waker());
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(()))
                }  
            })

        }).await
    }

    fn inner_blocking_flush(&self) -> Result<(), Error> {
        info!("blocking flush");
        while !self.tx.is_empty() {}
        Ok(())
    }

    pub fn inner_blocking_write(&self, buf: &[u8]) -> Result<usize, Error> {
        let mut writer = unsafe { self.tx.writer() };
        
        fn spin_wait(writer: &mut atomic_ring_buffer::Writer<'_>) -> (*mut u8, usize) {
            loop {
                let (data_ptr, n) = writer.push_buf();
                if n > 0 {
                    return (data_ptr, n)
                }
            }
        }

        let (data_ptr, n) = spin_wait(&mut writer);

        let len = n.min(buf.len());
        let data = unsafe { core::slice::from_raw_parts_mut(data_ptr, len) };
        data[..len].copy_from_slice(&buf[..len]);
        writer.push_done(len);
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        T::regs().ctl0.modify(|_, w| w.tbeie().set_bit());
        
        Ok(len)
        
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
    rx_waker: WakerRegistration,
    rx_writer: atomic_ring_buffer::Writer<'d>,
    tx_waker: WakerRegistration,
    tx_reader: atomic_ring_buffer::Reader<'d>,
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
            if !self.rx_writer.push_one(byte) {
                warn!("RX buffer full");
            }
            self.rx_waker.wake();
        }

        if stat0.tbe().bit_is_set() {
            if let Some(byte) = self.tx_reader.pop_one() {
                regs.data.write(|w| w.data().variant(byte.into()));
                self.tx_waker.wake();
            } else {
                // Disable the interrupt until we have something to transmit again
                regs.ctl0.modify(|_, w| w.tbeie().clear_bit());
            }
        }
        
    }
}
