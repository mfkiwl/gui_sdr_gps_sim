//! Bounded IQ buffer pool for decoupling the GPS generation thread from the
//! `HackRF` TX thread.
//!
//! # Design
//!
//! ```text
//! GPS thread                          TX thread
//! ----------                          ---------
//! producer.acquire()  ← free buffers ←  consumer.release(buf)
//! (fill buffer)
//! producer.enqueue(buf)  → queue  →   consumer.dequeue() → Some(buf)
//!                                     (submit to USB)
//!
//! Shutdown:
//! producer.shutdown()  → None sentinel → consumer.dequeue() returns None
//! ```
//!
//! Each buffer is a `Box<[i8]>` of exactly [`HACKRF_BUF_BYTES`] bytes holding
//! interleaved signed 8-bit I/Q samples.
//!
//! The `data_tx` channel carries `Option<Box<[i8]>>`; `None` is the shutdown
//! sentinel that tells the TX thread to stop after draining in-flight USB
//! transfers.
//!
//! [`HACKRF_BUF_BYTES`]: super::types::consts::HACKRF_BUF_BYTES

use crossbeam_channel::{bounded, Receiver, Sender};
use super::types::consts::HACKRF_BUF_BYTES;

// ── Public types ──────────────────────────────────────────────────────────────

/// A matched pair of producer/consumer endpoints backed by a shared buffer pool.
///
/// Created with [`IqFifo::new`].  Split the two endpoints and move each into
/// the appropriate thread.
pub struct IqFifo {
    /// GPS-thread side: acquire empty buffers, enqueue filled buffers.
    pub producer: IqProducer,
    /// TX-thread side: dequeue filled buffers, release consumed buffers.
    pub consumer: IqConsumer,
}

/// Producer (GPS thread) endpoint.
pub struct IqProducer {
    /// Receive empty buffers from the free pool.
    pub free_rx: Receiver<Box<[i8]>>,
    /// Send filled buffers to the TX thread.  `None` = shutdown sentinel.
    pub data_tx: Sender<Option<Box<[i8]>>>,
}

/// Consumer (TX thread) endpoint.
pub struct IqConsumer {
    /// Receive filled buffers from the GPS thread.
    pub data_rx: Receiver<Option<Box<[i8]>>>,
    /// Return consumed buffers to the free pool.
    pub free_tx: Sender<Box<[i8]>>,
}

// ── IqFifo ────────────────────────────────────────────────────────────────────

impl IqFifo {
    /// Create a pool of `n_bufs` empty IQ buffers, each `HACKRF_BUF_BYTES` bytes.
    ///
    /// The `data` channel capacity is also `n_bufs`, so the GPS thread will block
    /// once all buffers are filled and queued, providing natural back-pressure when
    /// the TX thread falls behind.
    ///
    /// # Typical value
    /// `n_bufs = 8` gives 8 × 262 144 bytes = 2 MB of buffering, which is
    /// approximately 330 ms of headroom at 3 MSPS × 2 bytes/sample.
    pub fn new(n_bufs: usize) -> Self {
        let (free_tx, free_rx) = bounded::<Box<[i8]>>(n_bufs);
        // Capacity n_bufs + 1: reserve one extra slot for the None shutdown
        // sentinel so that shutdown() never blocks even when all data buffers
        // are in flight.
        let (data_tx, data_rx) = bounded::<Option<Box<[i8]>>>(n_bufs + 1);

        // Pre-populate the free pool with zero-initialised buffers.
        for _ in 0..n_bufs {
            free_tx
                .send(vec![0i8; HACKRF_BUF_BYTES].into_boxed_slice())
                .expect("free pool channel unexpectedly closed during init");
        }

        Self {
            producer: IqProducer { free_rx, data_tx },
            consumer: IqConsumer { data_rx, free_tx },
        }
    }
}

// ── IqProducer (GPS thread) ───────────────────────────────────────────────────

impl IqProducer {
    /// Acquire an empty buffer from the free pool.
    ///
    /// **Blocks** until a buffer is available.  This provides back-pressure:
    /// if all `n_bufs` buffers are queued waiting for USB submission, the GPS
    /// thread pauses rather than allocating unbounded memory.
    pub fn acquire(&self) -> Box<[i8]> {
        self.free_rx.recv().expect("FIFO free pool closed unexpectedly")
    }

    /// Submit a filled buffer to the TX thread.
    pub fn enqueue(&self, buf: Box<[i8]>) {
        self.data_tx
            .send(Some(buf))
            .expect("FIFO data channel closed unexpectedly");
    }

    /// Signal the TX thread to shut down after draining all queued buffers.
    ///
    /// Sends the `None` sentinel.  The TX thread will call [`IqConsumer::dequeue`]
    /// until it returns `None`, then stop.
    pub fn shutdown(&self) {
        // Ignore send error — TX thread may have already exited.
        let _sent: Result<(), _> = self.data_tx.send(None);
    }
}

// ── IqConsumer (TX thread) ───────────────────────────────────────────────────

impl IqConsumer {
    /// Receive the next filled buffer, blocking until one is available.
    ///
    /// Returns `None` when the GPS thread has called [`IqProducer::shutdown`]
    /// and all previously enqueued buffers have been returned.  The TX thread
    /// should treat `None` as its stop signal.
    pub fn dequeue(&self) -> Option<Box<[i8]>> {
        // recv() returns Err only if the sender has been dropped.
        // In that case, treat it as shutdown (equivalent to receiving None).
        self.data_rx.recv().ok().flatten()
    }

    /// Return a consumed buffer to the free pool so the GPS thread can reuse it.
    pub fn release(&self, buf: Box<[i8]>) {
        // Ignore error — GPS thread may have already exited.
        let _sent: Result<(), _> = self.free_tx.send(buf);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::unwrap_used, reason = "test asserts is_some() before unwrap")]
    fn buffer_size_correct() {
        let fifo = IqFifo::new(2);
        let buf = fifo.producer.acquire();
        assert_eq!(buf.len(), HACKRF_BUF_BYTES, "buffer length mismatch");
        fifo.producer.enqueue(buf);
        let received = fifo.consumer.dequeue();
        assert!(received.is_some());
        fifo.consumer.release(received.unwrap());
    }

    #[test]
    fn shutdown_sentinel_terminates_consumer() {
        let fifo = IqFifo::new(4);
        fifo.producer.shutdown();
        // The very first dequeue should return None (the shutdown sentinel).
        let result = fifo.consumer.dequeue();
        assert!(result.is_none(), "expected None after shutdown");
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "indexing buf[0] in tests is safe; buffer is non-empty by construction")]
    fn roundtrip_multiple_buffers() {
        let n = 4;
        let fifo = IqFifo::new(n);

        // Fill and enqueue all buffers.
        for i in 0..n {
            let mut buf = fifo.producer.acquire();
            buf[0] = i as i8; // tag the buffer
            fifo.producer.enqueue(buf);
        }
        fifo.producer.shutdown();

        // Dequeue and verify order + sentinel.
        for i in 0..n {
            let buf = fifo.consumer.dequeue().expect("expected Some");
            assert_eq!(buf[0], i as i8, "buffer order mismatch at index {i}");
            fifo.consumer.release(buf);
        }
        assert!(fifo.consumer.dequeue().is_none(), "expected None sentinel");
    }
}
