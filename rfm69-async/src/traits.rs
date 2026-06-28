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
    /// The `Transceiver` has no active-recovery implementation. Returned by
    /// the default [`Transceiver::recover`] so the `Runner` keeps the link
    /// `Down` rather than treating an unimplemented recovery as success.
    RecoverUnsupported,
}

// `async fn in trait` deliberately leaves the Send-bound on the returned
// futures unspecified. embassy on the targets this crate supports is
// single-executor, so Send isn't needed; if a future user runs across
// executor threads they can desugar a wrapper trait that adds the bound.
#[expect(async_fn_in_trait)]
pub trait Transceiver {
    async fn send(&mut self, packet: &Packet) -> Result<(), TrxError>;
    async fn recv(&mut self) -> Result<Packet, TrxError>;

    /// Hook the `Runner` invokes when the link transitions to
    /// [`LinkState::Down`](crate::LinkState::Down) — a streak of consecutive
    /// `TrxError`s on any radio operation.
    ///
    /// Implementations should drive the radio back to a usable state (e.g.
    /// pulse `RESET` and re-apply a `config::*` helper). On `Ok(())` the
    /// `Runner` resumes normal operation; the next successful `send` / `recv`
    /// then flips the link back to `Up`. On `Err(_)` the `Runner` keeps the
    /// link `Down` and re-invokes `recover` after a backoff configured on
    /// `MacTiming`.
    ///
    /// Default: returns [`TrxError::RecoverUnsupported`]. A radio that doesn't
    /// override `recover` therefore stays `Down` once the link drops — the
    /// `Runner` keeps retrying `recover` every `MacTiming::recover_backoff`
    /// but never makes progress. Override this to opt into active recovery.
    async fn recover(&mut self) -> Result<(), TrxError> {
        Err(TrxError::RecoverUnsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Transceiver` that doesn't override `recover`, so it exercises the
    /// trait's default. `send` / `recv` are never polled by the test.
    struct NoRecover;
    impl Transceiver for NoRecover {
        async fn send(&mut self, _packet: &Packet) -> Result<(), TrxError> {
            unreachable!()
        }
        async fn recv(&mut self) -> Result<Packet, TrxError> {
            unreachable!()
        }
    }

    #[test]
    fn default_recover_reports_unsupported() {
        // Guards the Runner contract: an impl without active recovery must
        // surface an error so the link stays `Down`, not a silent `Ok`.
        let mut trx = NoRecover;
        let result = futures::executor::block_on(trx.recover());
        assert!(matches!(result, Err(TrxError::RecoverUnsupported)));
    }
}
