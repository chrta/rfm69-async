// SPDX-License-Identifier: AGPL-3.0-only

use crate::Packet;

/// Stable, lossy error enum for the transport-agnostic [`Transceiver`] trait.
///
/// The driver's parametric [`crate::Error`] preserves the underlying SPI /
/// GPIO error types, which is useful for users that hold a concrete
/// `Rfm69<...>` but does not survive a `dyn` boundary or a generic that
/// abstracts over the backing radio. `TrxError` collapses those variants
/// into a fixed vocabulary for use across the `Transceiver` boundary.
///
/// **The lossy shape is deliberate** (the alternative was to make
/// `TrxError` parametric over the SPI / RESET / DIO0 error types). Reasons:
///
/// - Mirrors the `embassy-net::Stack` pattern — high-level Stack APIs
///   should not leak the underlying driver's error types across the
///   abstraction they were created to draw.
/// - Keeps `Stack<'a>`, `Runner<'a, TRX>`, and the channel slot type
///   free of an extra error-type generic, so user code threading `Stack`
///   around stays terse.
/// - Future multi-radio backends get to share one stable error
///   vocabulary; user code keeps working when swapping backings.
/// - Debuggability is preserved by the [`Runner`](crate::Runner) logging
///   the underlying error chain via the internal `error!` macro before
///   handing the collapsed `TrxError` to the user.
///
/// Power users who want the full parametric error chain can call the
/// inherent `Rfm69::send` / `Rfm69::recv` directly, which still return
/// `Error<SPI, RESET, DIO0>` — the lossy collapse only happens at the
/// `Transceiver` boundary.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TrxError {
    /// Reading the version register failed or returned an unexpected value
    /// during configuration / reset.
    TrxNotFound,
    /// The reset GPIO write failed.
    Reset,
    /// An SPI bus transaction failed.
    Spi,
    /// A DIO0 GPIO read or interrupt-wait failed.
    Gpio,
    /// A configuration step (e.g. sync-word size) was rejected.
    Config,
    /// A packet's framing was invalid (length out of range, etc.).
    WrongPacketFormat,
}

// `async fn in trait` deliberately leaves the Send-bound on the returned
// futures unspecified. embassy on the targets this crate supports is
// single-executor, so Send isn't needed; if a future user runs across
// executor threads they can desugar a wrapper trait that adds the bound.
#[allow(async_fn_in_trait)]
pub trait Transceiver {
    async fn send(&mut self, packet: &Packet) -> Result<(), TrxError>;
    async fn recv(&mut self) -> Result<Packet, TrxError>;
}
