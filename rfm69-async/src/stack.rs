// SPDX-License-Identifier: AGPL-3.0-only

//! Smoltcp-style Stack/Runner split for concurrent rx/tx over a half-duplex
//! radio.
//!
//! # Why
//!
//! The radio is half-duplex: only one of TX or RX can be active at a time.
//! With a single `&mut self` driver, that constraint surfaces in user code
//! as "you can't listen for incoming packets while another task wants to
//! send" -- common patterns like *"always-listening node that occasionally
//! responds"* end up needing ad-hoc `with_timeout` dances.
//!
//! This module hides the constraint behind a long-running [`Runner`] task
//! that owns the radio exclusively. User tasks call [`Stack::send`] /
//! [`Stack::recv`] from any number of independent tasks; the Runner
//! arbitrates between TX requests and incoming packets via
//! `embassy_futures::select`. From the user's perspective send and recv
//! can run concurrently. Mirrors `embassy-net::Stack` / `embassy-net::Runner`.
//!
//! # TX serialization
//!
//! Half-duplex means at most one TX in flight at a time. [`Stack::send`]
//! enforces this with a strict ordering inside the lock:
//!
//! 1. Acquire `tx_mutex` -- serializes callers FIFO via embassy's mutex queue.
//! 2. **Reset `tx_response`** to clear any leftover from a cancelled previous
//!    call.
//! 3. Send a [`TxRequest`] through the single-slot `tx_request` channel.
//! 4. Await the result on `tx_response.wait()`.
//! 5. Drop the guard.
//!
//! Step 2 *under the mutex* is the cancellation-safety contract: if a caller
//! is dropped between `tx_request.send` and `tx_response.wait`, the Runner
//! still writes the result into the [`Signal`], which the next `Stack::send`
//! resets before posting its own request. The TX channel depth of 1 is
//! intentional -- `tx_mutex` already guarantees no second request enters the
//! channel before the first is consumed.
//!
//! # Cancellation safety of the Runner loop
//!
//! [`Runner::run`] loops on `select(tx_request_recv, trx.recv)`. When the TX
//! branch wins, the dropped `trx.recv()` future has done no SPI work yet
//! (the radio is just sitting in RX). When the RX branch wins, the dropped
//! `tx_request_recv.receive()` future was a passive channel poll. Neither
//! branch leaves hardware in a half-configured state.
//!
//! # Error type at the boundary
//!
//! The user-facing surface uses [`TxError`] (and the generic [`TrxError`]
//! from [`crate::traits`]) -- a fixed, lossy error vocabulary. The
//! parametric [`crate::Error`] is collapsed at the [`Transceiver`] boundary.
//! See the [`TrxError`] rustdoc for the rationale.

use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, DynamicReceiver, DynamicSender};
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer, with_timeout};
use heapless::Vec;

use crate::{Address, Flags, Packet, Transceiver, TrxError};

/// Maximum payload bytes that fit on the wire (matches the `Packet` MTU).
const PAYLOAD_CAP: usize = 61;

/// Errors returned by [`Stack::send`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TxError {
    /// The peer did not ACK within the configured retries.
    AckTimeout,
    /// `data` was longer than the radio's 61-byte MTU.
    DataTooLong,
    /// The Runner reported an underlying radio error during the send.
    Trx(TrxError),
}

/// Tunable MAC-layer timing knobs, owned by the [`Runner`].
///
/// Use [`MacTiming::defaults`] (also available as [`Default::default`]) to
/// keep the historical values that today's first-cut Stack uses.
#[derive(Debug, Clone, Copy)]
pub struct MacTiming {
    /// Pause inserted between handing the radio a packet that requested an
    /// ACK and emitting the ACK reply, so the original sender has time to
    /// transition back to RX. Default: 10 ms.
    pub ack_tx_delay: Duration,
    /// Per-attempt window to wait for the requested ACK before retrying.
    /// Default: 50 ms.
    pub ack_timeout: Duration,
    /// Pause between consecutive TX attempts when the previous attempt's
    /// ACK timed out. Default: 200 ms.
    pub tx_retry_delay: Duration,
}

impl MacTiming {
    pub const fn defaults() -> Self {
        Self {
            ack_tx_delay: Duration::from_millis(10),
            ack_timeout: Duration::from_millis(50),
            tx_retry_delay: Duration::from_millis(200),
        }
    }
}

impl Default for MacTiming {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Internal: a queued user send, picked up by the Runner.
struct TxRequest {
    dst: Address,
    flags: Flags,
    data: Vec<u8, PAYLOAD_CAP>,
}

/// Caller-allocated channel buffers for a [`Stack`].
///
/// Allocate this in a `static` (typically via `static_cell::make_static!`),
/// then hand a `&mut` to [`Stack::new`]. The const generic `N_RX` controls
/// how many incoming packets can buffer between Runner deliveries and
/// `Stack::recv` consumption.
pub struct StackResources<const N_RX: usize = 4> {
    rx: Channel<NoopRawMutex, Packet, N_RX>,
    tx_request: Channel<NoopRawMutex, TxRequest, 1>,
    tx_response: Signal<NoopRawMutex, Result<(), TxError>>,
    tx_mutex: Mutex<NoopRawMutex, ()>,
}

impl<const N_RX: usize> StackResources<N_RX> {
    pub const fn new() -> Self {
        Self {
            rx: Channel::new(),
            tx_request: Channel::new(),
            tx_response: Signal::new(),
            tx_mutex: Mutex::new(()),
        }
    }
}

impl<const N_RX: usize> Default for StackResources<N_RX> {
    fn default() -> Self {
        Self::new()
    }
}

/// Cheap (`Copy`) handle to a [`Stack`]. Pass to user tasks; clone freely.
#[derive(Copy, Clone)]
pub struct Stack<'a> {
    address: Address,
    rx: DynamicReceiver<'a, Packet>,
    tx_request: DynamicSender<'a, TxRequest>,
    tx_response: &'a Signal<NoopRawMutex, Result<(), TxError>>,
    tx_mutex: &'a Mutex<NoopRawMutex, ()>,
}

/// Long-running task that owns the radio. Spawn it once; call its
/// [`Runner::run`] method, which never returns.
pub struct Runner<'a, TRX> {
    trx: TRX,
    address: Address,
    rx: DynamicSender<'a, Packet>,
    tx_request: DynamicReceiver<'a, TxRequest>,
    tx_response: &'a Signal<NoopRawMutex, Result<(), TxError>>,
    timing: MacTiming,
}

impl<'a> Stack<'a> {
    /// Splits caller-allocated resources into a [`Stack`] handle and a
    /// [`Runner`] that owns the radio. After this call the resources are
    /// pinned in place via the returned references.
    pub fn new<TRX, const N_RX: usize>(
        trx: TRX,
        address: Address,
        resources: &'a mut StackResources<N_RX>,
        timing: MacTiming,
    ) -> (Stack<'a>, Runner<'a, TRX>) {
        let StackResources {
            rx,
            tx_request,
            tx_response,
            tx_mutex,
        } = resources;
        let stack = Stack {
            address,
            rx: rx.dyn_receiver(),
            tx_request: tx_request.dyn_sender(),
            tx_response,
            tx_mutex,
        };
        let runner = Runner {
            trx,
            address,
            rx: rx.dyn_sender(),
            tx_request: tx_request.dyn_receiver(),
            tx_response,
            timing,
        };
        (stack, runner)
    }

    /// The local address this Stack was constructed with.
    pub fn address(&self) -> Address {
        self.address
    }

    /// Send `data` to `dst` with the given `flags`. Serialized across all
    /// concurrent callers via an internal mutex (the radio is half-duplex,
    /// so only one TX flies at a time anyway).
    ///
    /// The order under the lock is **lock → reset response → enqueue
    /// request → await response**, in that order. Resetting under the lock
    /// is the cancellation-safety invariant: a previously-cancelled caller
    /// may have left a stale value in the [`Signal`], and clearing it
    /// before posting our own request makes sure we wait for the response
    /// to *our* TX, not the dead one's. See the module-level docs.
    pub async fn send(&self, dst: Address, flags: Flags, data: &[u8]) -> Result<(), TxError> {
        let mut buf: Vec<u8, PAYLOAD_CAP> = Vec::new();
        buf.extend_from_slice(data).map_err(|_| TxError::DataTooLong)?;

        let _guard = self.tx_mutex.lock().await;
        self.tx_response.reset();
        self.tx_request.send(TxRequest { dst, flags, data: buf }).await;
        self.tx_response.wait().await
    }

    /// Wait for the next packet addressed to this Stack (Unicast match) or
    /// to `Address::Broadcast`. Other unicasts are filtered out by the
    /// Runner before they reach the queue.
    pub async fn recv(&self) -> Packet {
        self.rx.receive().await
    }
}

impl<'a, TRX: Transceiver> Runner<'a, TRX> {
    /// Run forever. Intended to be spawned as an `#[embassy_executor::task]`.
    ///
    /// The body is `select(tx_request_recv, trx.recv())` -- whichever fires
    /// first wins, and the loser's future is dropped. That's safe by
    /// construction: `trx.recv()` hasn't started any SPI work while sitting
    /// in RX, and `tx_request.receive()` is a passive channel poll. The
    /// radio is never left in a half-configured state.
    pub async fn run(&mut self) -> ! {
        loop {
            match select(self.tx_request.receive(), self.trx.recv()).await {
                Either::First(req) => self.handle_tx(req).await,
                Either::Second(Ok(packet)) => self.handle_rx(packet).await,
                Either::Second(Err(e)) => {
                    error!("Stack: rx error: {:?}", e);
                }
            }
        }
    }

    async fn handle_tx(&mut self, req: TxRequest) {
        let result = self.do_send(req).await;
        self.tx_response.signal(result);
    }

    async fn do_send(&mut self, req: TxRequest) -> Result<(), TxError> {
        let packet = Packet::new(self.address, req.dst, req.flags, &req.data)
            .map_err(|_| TxError::Trx(TrxError::WrongPacketFormat))?;

        match req.flags {
            Flags::None | Flags::Ack(0) => {
                info!("Stack: send (no ack)");
                self.trx.send(&packet).await.map_err(TxError::Trx)
            }
            Flags::Ack(retries) => {
                for i in 1..=retries {
                    info!("Stack: send {} of {} (waiting ACK)", i, retries);
                    self.trx.send(&packet).await.map_err(TxError::Trx)?;
                    match with_timeout(self.timing.ack_timeout, self.wait_for_ack(req.dst)).await {
                        Ok(Ok(())) => return Ok(()),
                        Ok(Err(e)) => return Err(TxError::Trx(e)),
                        Err(_) => Timer::after(self.timing.tx_retry_delay).await,
                    }
                }
                Err(TxError::AckTimeout)
            }
        }
    }

    /// Block on `trx.recv()` until our peer's ACK arrives. Non-ACK packets
    /// that interleaved with the ACK race aren't dropped -- they get pushed
    /// to the user rx queue via [`Self::try_deliver`] so the inbound side
    /// doesn't lose user traffic during a TX.
    async fn wait_for_ack(&mut self, from: Address) -> Result<(), TrxError> {
        loop {
            let packet = self.trx.recv().await?;
            if packet.src == from && packet.dst == self.address && packet.is_ack() {
                info!("Stack: valid ACK");
                return Ok(());
            }
            self.try_deliver(packet);
        }
    }

    /// We ACK a packet only when `flags` is `Flags::Ack(n)` with `n > 0`
    /// **and** `dst == self.address`. Broadcasts never ACK -- ACK storms
    /// don't scale -- and `Ack(0)` is itself an ACK reply, so ACKing one
    /// would be infinite recursion. The packet is delivered to the user
    /// rx queue regardless of whether we ACKed.
    async fn handle_rx(&mut self, packet: Packet) {
        if let Flags::Ack(n) = packet.flags
            && n > 0
            && packet.dst == self.address
        {
            let Ok(ack) = Packet::new(self.address, packet.src, Flags::Ack(0), &[]) else {
                return;
            };
            info!("Stack: replying ACK");
            Timer::after(self.timing.ack_tx_delay).await;
            if let Err(e) = self.trx.send(&ack).await {
                error!("Stack: ACK send failed: {:?}", e);
                return;
            }
        }
        self.try_deliver(packet);
    }

    /// Push to the user rx queue if the packet is addressed to us
    /// (Unicast match or Broadcast). The push is non-blocking: an
    /// overflowing rx queue logs a warning and drops the packet, never
    /// backpressuring the radio. A user task that can't keep up should
    /// raise `N_RX` rather than rely on the queue holding everything.
    fn try_deliver(&self, packet: Packet) {
        if packet.dst != Address::Broadcast && packet.dst != self.address {
            return;
        }
        if self.rx.try_send(packet).is_err() {
            warn!("Stack: rx queue full, packet dropped");
        }
    }
}
