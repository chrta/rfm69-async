// SPDX-License-Identifier: AGPL-3.0-only

//! `no_std` async driver for the HopeRF / Semtech **RFM69** sub-GHz FSK
//! transceiver. Generic over [`embedded-hal`](embedded_hal) and
//! [`embedded-hal-async`](embedded_hal_async) 1.0 traits, so the same
//! driver runs on any MCU whose HAL provides them.
//!
//! # Two API layers
//!
//! The crate exposes two ways to drive a radio. Pick whichever fits.
//!
//! ### 1. Low-level: [`Rfm69`]
//!
//! [`Rfm69`] is the bare transceiver: register setters, `send`, `recv`,
//! and an [`Rfm69::reset`] that does the post-power-up version check.
//! Use it directly if your application owns the radio from a single
//! task and you want minimal indirection.
//!
//! ### 2. High-level: [`Stack`] / [`Runner`] (feature `embassy`)
//!
//! For applications where multiple tasks want to share one radio — the
//! common embedded pattern of *"always-listening node that occasionally
//! sends"* — [`Stack`] and [`Runner`] provide an `embassy-net`-style
//! split:
//!
//! - A long-running [`Runner`] task owns the radio exclusively and
//!   arbitrates between TX requests and incoming packets via
//!   `embassy_futures::select`.
//! - User tasks hold cheap [`Stack`] handles (Copy, clone freely) and
//!   call [`Stack::send`] / [`Stack::recv`] concurrently. The Runner
//!   serializes the half-duplex bus underneath.
//! - A small MAC layer above the wire — [`Flags::Ack`] retries with
//!   [`MacTiming`] knobs, broadcast filtering, RSSI capture — is
//!   provided by the Runner so user code stays simple.
//! - Hardware health surfaces as [`LinkState`]: a streak of
//!   [`LINK_DOWN_STREAK`] consecutive `TrxError`s flips the link to
//!   `Down`; the next success flips it back. Observe via
//!   [`Stack::wait_link_up`] / [`Stack::wait_link_down`].
//!
//! See `examples/rp/src/bin/concurrent_demo.rs` in the repository for
//! the canonical Stack/Runner setup pattern.
//!
//! # The `Transceiver` boundary
//!
//! Both layers meet at the [`Transceiver`] trait — a small async trait
//! with `send` / `recv` and a fixed [`TrxError`] vocabulary.
//! [`Stack`] / [`Runner`] are generic over `TRX: Transceiver`, so the
//! Stack is reusable against any backing radio (or a mock) that
//! implements the trait, not just `Rfm69`.
//!
//! # Quick start
//!
//! Construction follows the pattern below. The full HAL plumbing
//! (SPI device, pin modes, async DMA setup) is HAL-specific; see the
//! `examples/rp/` binaries for runnable code on the RP2040.
//!
//! ```ignore
//! use rfm69_async::{config, Address, Flags, Rfm69, Stack, StackResources, MacTiming};
//!
//! // 1. Construct the bare driver from your HAL's SPI / GPIO / delay.
//! let mut rfm = Rfm69::new(spi, reset_pin, Some(dio0_pin), delay);
//!
//! // 2. Apply a preset configuration in place. The same call can be re-issued
//! //    later (e.g. from a `Transceiver::recover` impl) to re-initialize after
//! //    a hardware fault.
//! config::my_defaults(&mut rfm, /* network_id */ 0x42, /* freq Hz */ 868_000_000).await?;
//!
//! // 3. Split into a Stack handle and a Runner task.
//! static mut RES: StackResources<4> = StackResources::new();
//! let (stack, mut runner) = Stack::new(rfm, Address::Unicast(1), unsafe { &mut RES }, MacTiming::default());
//!
//! // 4. Spawn the runner (here in the same task via embassy_futures::join).
//! //    In real apps, spawn it as an embassy_executor task instead.
//! embassy_futures::join::join(
//!     runner.run(),
//!     async {
//!         stack.send(Address::Unicast(2), Flags::Ack(3), b"hello").await?;
//!         let pkt = stack.recv().await;
//!         Ok::<_, rfm69_async::TxError>(())
//!     },
//! ).await;
//! ```
//!
//! For the lower-level direct path, swap step 3+4 for `rfm.send(&packet).await?`
//! / `rfm.recv().await?` calls in your own loop.
//!
//! # Optional DIO0 pin
//!
//! [`Rfm69::new`] takes `dio0: Option<DIO0>`. With a pin connected, the
//! driver awaits hardware-driven `PacketSent` / `PayloadReady` events
//! via [`Wait::wait_for_high`](embedded_hal_async::digital::Wait::wait_for_high)
//! — preferred. With `None::<SomePin>` the driver falls back to
//! polling `IrqFlags2`. Both modes are functionally equivalent; the
//! pin-driven path is cheaper on the CPU.
//!
//! # Cargo features
//!
//! All features are off by default — opt in to what you need.
//!
//! - **`embassy`** — pulls in `embassy-time` / `embassy-sync` /
//!   `embassy-futures` and enables the [`Stack`] / [`Runner`] surface
//!   (plus [`MacTiming`], [`LinkState`], [`StackResources`], [`TxError`]).
//!   Required for any of the high-level API.
//! - **`log`** — routes the driver's internal `info!` / `debug!` /
//!   `warn!` / `error!` macros through the
//!   [`log`](https://docs.rs/log) facade. Use this when piping output
//!   over USB CDC via `embassy-usb-logger`, or any other `log`-based
//!   sink.
//! - **`defmt`** — derives [`defmt::Format`] on the public error /
//!   address / packet types and routes the driver's logging through
//!   [`defmt`](https://docs.rs/defmt). Use for `defmt-rtt` over a
//!   debug probe.
//!
//! `log` and `defmt` are not mutually exclusive — enabling both fans
//! the driver's log output to both backends. With neither enabled the
//! logging macros expand to no-ops that still consume their arguments
//! (so `unused_variables` warnings don't fire on debug-only locals).
//!
//! # Errors
//!
//! Two error types deliberately exist:
//!
//! - [`Error`]`<SPI, RESET, DIO0>` — the parametric error returned by
//!   the inherent `Rfm69::*` methods. Preserves the underlying HAL's
//!   error types verbatim, useful when you want to inspect the cause.
//! - [`TrxError`] — a fixed, lossy enum at the [`Transceiver`] /
//!   [`Stack`] boundary. The parametric error is collapsed here so
//!   `Stack` / `Runner` don't need an extra error-type generic.
//!   Mirrors the `embassy-net::Stack` pattern.
//!
//! # MSRV
//!
//! Currently 1.87, set by the `heapless = "0.9"` dependency. The crate
//! itself uses no nightly features and compiles on any stable Rust at
//! or above that floor.

#![no_std]

#[macro_use]
mod fmt;

mod address;
pub mod config;
mod error;
mod flags;
mod packet;
pub mod registers;
mod rfm;
mod traits;

#[cfg(feature = "embassy")]
mod stack;

pub use address::Address;
pub use error::Error;
pub use flags::Flags;
pub use packet::Packet;
pub use rfm::Rfm69;
#[cfg(feature = "embassy")]
pub use stack::{LINK_DOWN_STREAK, LinkState, MacTiming, Runner, Stack, StackResources, TxError};
pub use traits::{Transceiver, TrxError};
