// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test fixtures for the Stack/Runner integration test bins.
//!
//! Each `tests/stack_*.rs` file is a *separate* test binary (Cargo compiles
//! one process per top-level `tests/*.rs`), so global state — the static
//! `embassy_time::MockDriver`, the `generic-queue-16` timer queue —
//! starts fresh per test. That isolation is why the suite is split into
//! many small files instead of one big one.

#![allow(dead_code)]

use core::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use embassy_time::Duration as EmbassyDuration;
use futures::FutureExt;
use futures::executor::block_on;
use rfm69_async::{Address, Flags, MacTiming, Packet, Stack, StackResources, Transceiver, TrxError};

/// Hand-rolled Transceiver mock. Holds an inbox of packets the test wants
/// the Runner to "receive" and an outbox of packets the Runner sends.
///
/// Single-threaded by construction — tests use a single-threaded futures
/// executor, so the `RefCell` interior is sound and we don't need an `Arc`
/// or atomics.
#[derive(Clone, Default)]
pub struct MockTrx {
    inbox: Rc<RefCell<VecDeque<Result<Packet, TrxError>>>>,
    outbox: Rc<RefCell<Vec<Packet>>>,
    recover_queue: Rc<RefCell<VecDeque<Result<(), TrxError>>>>,
    recover_calls: Rc<RefCell<u32>>,
}

impl MockTrx {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a packet for the Runner to receive on its next `recv` poll.
    pub fn inject(&self, packet: Packet) {
        self.inbox.borrow_mut().push_back(Ok(packet));
    }

    /// Queue a `recv` error for the Runner to surface on its next poll.
    /// Used by the LinkState integration tests to drive the Runner's
    /// consecutive-error streak.
    pub fn inject_err(&self, err: TrxError) {
        self.inbox.borrow_mut().push_back(Err(err));
    }

    /// Make the next `recover` call succeed. Recovery tests use this to
    /// verify the Runner clears `LinkState::Down`.
    pub fn inject_recover_ok(&self) {
        self.recover_queue.borrow_mut().push_back(Ok(()));
    }

    /// Make the next `recover` call fail with the given error.
    pub fn inject_recover_err(&self, err: TrxError) {
        self.recover_queue.borrow_mut().push_back(Err(err));
    }

    /// Total number of times the Runner has invoked `recover` so far.
    pub fn recover_calls(&self) -> u32 {
        *self.recover_calls.borrow()
    }

    /// Snapshot of packets the Runner has sent so far.
    pub fn sent(&self) -> Vec<Packet> {
        self.outbox.borrow().clone()
    }
}

impl Transceiver for MockTrx {
    async fn send(&mut self, packet: &Packet) -> Result<(), TrxError> {
        self.outbox.borrow_mut().push(packet.clone());
        Ok(())
    }

    async fn recv(&mut self) -> Result<Packet, TrxError> {
        loop {
            if let Some(item) = self.inbox.borrow_mut().pop_front() {
                return item;
            }
            // Yield so the test driver and the user-side Stack future get a
            // chance to run and inject something. `pending!` doesn't wake
            // its waker, so use the local `yield_now` instead.
            yield_now().await;
        }
    }

    async fn recover(&mut self) -> Result<(), TrxError> {
        *self.recover_calls.borrow_mut() += 1;
        // Default: recover fails so existing Down-streak tests see a sticky
        // Down. Tests that exercise the recovery happy path queue up Ok
        // responses via `inject_recover_ok`.
        self.recover_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(Err(TrxError::Reset))
    }
}

/// Build a fresh `Packet` for tests. Wraps `Packet::new` to keep the test
/// bodies short.
pub fn pkt(src: Address, dst: Address, flags: Flags, data: &[u8]) -> Packet {
    Packet::new(src, dst, flags, data).unwrap()
}

/// A pacer future that advances `MockDriver` by `step_ms` per yield, for up
/// to `steps` yields. Used to drive time forward concurrently with the
/// future under test, so the Runner's `Timer::after` / `with_timeout` calls
/// resolve. When this future ends (after enough virtual time has passed),
/// it counts as "the future under test didn't terminate" — the test
/// `select!`s on both and panics on this branch winning.
pub async fn pace_time(steps: usize, step_ms: u64) {
    let driver = embassy_time::MockDriver::get();
    for _ in 0..steps {
        driver.advance(EmbassyDuration::from_millis(step_ms));
        yield_now().await;
    }
}

/// Yield once, scheduling ourselves for re-poll. `futures::pending!()` does
/// NOT wake its waker, so using it here would deadlock `block_on` (no future
/// is woken, so the executor parks). Hand-rolled yield that actually wakes.
pub fn yield_now() -> impl core::future::Future<Output = ()> {
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll};

    struct Yield(bool);
    impl Future for Yield {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    Yield(false)
}

/// Run a test alongside the Runner. The body returns a future to await; the
/// Runner is dropped when that future resolves.
///
/// Each test binary owns the global static `MockDriver` for itself, so no
/// reset / serialization is needed between binaries.
pub fn run_test<Body>(body: Body)
where
    Body: for<'a> AsyncFnOnce(Stack<'a>, &'a MockTrx),
{
    let mut resources = StackResources::<4>::new();
    let trx = MockTrx::new();

    let timing = MacTiming {
        ack_tx_delay: EmbassyDuration::from_millis(1),
        ack_timeout: EmbassyDuration::from_millis(50),
        tx_retry_delay: EmbassyDuration::from_millis(10),
        // Long enough that the Runner is parked across `pace_time(50, 1)`'s
        // 50 ms window, so a Down -> recover -> Err -> backoff cycle doesn't
        // overlap a test's observation window.
        recover_backoff: EmbassyDuration::from_millis(500),
    };
    let (stack, mut runner) = Stack::new(trx.clone(), Address::Unicast(1), &mut resources, timing);
    let trx_ref = &trx;

    block_on(async move {
        let test = body(stack, trx_ref).fuse();
        let runner = runner.run().fuse();
        futures::pin_mut!(test);
        futures::pin_mut!(runner);
        futures::select! {
            _ = test => {},
            _ = runner => unreachable!("runner exited"),
        }
    });
}
