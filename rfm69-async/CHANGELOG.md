# Changelog

All notable changes to the `rfm69-async` crate are recorded here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Stack / Runner API.** New high-level surface in the `stack` module
  (gated on the `embassy` feature) modelled after `embassy-net`. A
  long-running `Runner` task owns the radio; user tasks hold cheap
  `Stack` handles and call `send` / `recv` concurrently. The Runner
  serializes the half-duplex bus, applies an ACK retry/timeout MAC, and
  filters incoming packets by address before delivering them.
- **`LinkState` observable** on `Stack`: a streak of three consecutive
  `TrxError`s on any radio operation flips the link to `Down`; the next
  success flips it back. Surface via `Stack::is_link_up`,
  `Stack::link_state`, `Stack::wait_link_up`, `Stack::wait_link_down`.
- **`Transceiver::recover` hook + active recovery in `Runner`.** New
  trait method (default `Ok(())` no-op) the `Runner` calls when the
  link transitions to `Down`. On `Ok(())` the Runner resumes normal
  ops; on `Err(_)` it backs off (`MacTiming::recover_backoff`, default
  500 ms) and retries. Override `recover` in your `Transceiver` impl to
  re-pulse `RESET` and re-apply a `config::*` helper.
- **`registers` module is now public.** The typed wrappers the
  `Rfm69` setters take (`OpMode`, `Modulation`, `RxBw`, `PacketConfig`,
  `LnaConfig`, `FifoMode`, `ContinuousDagc`, …) are exposed under
  `rfm69_async::registers::*` so callers can write their own
  configuration helpers when neither `config::my_defaults` nor
  `config::low_power_lab_defaults` fits.
- **`Transceiver` trait** as the abstraction boundary between the
  high-level Stack and the underlying radio. `Stack` / `Runner` are
  generic over `TRX: Transceiver`, so they're reusable against any
  backing radio (or a mock) implementing the trait.
- **`TrxError`** — fixed lossy error vocabulary at the `Transceiver`
  boundary. The parametric `Error<SPI, RESET, DIO0>` is preserved on
  the inherent `Rfm69::*` methods for callers that want the underlying
  HAL error chain.
- **Cargo features** (no defaults — opt-in):
  - `embassy` — enables the Stack/Runner surface (pulls in
    `embassy-time`, `embassy-sync`, `embassy-futures`).
  - `log` — routes the driver's internal `info!` / `debug!` / `warn!`
    / `error!` macros through the [`log`](https://docs.rs/log) facade.
  - `defmt` — derives `defmt::Format` on the public error / address /
    packet / link-state types and routes the driver's logging through
    [`defmt`](https://docs.rs/defmt). Both `log` and `defmt` can be
    enabled simultaneously.
- **Optional DIO0 pin.** `Rfm69::new` now accepts `dio0: Option<DIO0>`.
  With a pin, the driver awaits `PacketSent` / `PayloadReady` events
  via `Wait::wait_for_high`. With `None`, it falls back to polling
  `IrqFlags2`.
- **Test infrastructure.** Host unit tests for the wire codecs
  (`Address`, `Flags`, `Packet`), Stack/Runner integration tests under
  `tests/` (using a `MockTrx` `Transceiver` impl driven by
  `embassy_time::MockDriver`), and SPI-level register-sequence tests
  in `tests/spi_protocol.rs` (using `embedded-hal-mock`) covering the
  reset, single read/write, multi-register write, and send/recv happy
  paths in both DIO0-connected and IrqFlags2-polling modes.
- **Crate-level rustdoc** on `lib.rs` covering the two API layers, the
  `Transceiver` boundary, optional DIO0, the cargo feature matrix, and
  the two error types. `[package.metadata.docs.rs] all-features = true`
  so docs.rs renders the embassy-gated surface.
- **`concurrent_demo` example** (`examples/rp/`) showing the canonical
  Stack/Runner setup pattern with three independent embassy tasks
  (runner / rx / tx) and a `Stack<'static>`.

### Changed

- **Config helpers take `&mut Rfm69` instead of consuming by value.**
  `config::my_defaults` / `config::low_power_lab_defaults` now have the
  signature `(rfm: &mut Rfm69<...>, network_id, frequency) -> Result<(), Error>`
  rather than `(rfm) -> Result<Rfm69, Error>`. Migration: drop the
  consume-and-rebind, pass a mutable reference instead.
  ```rust
  // Before:
  let rfm = config::my_defaults(Rfm69::new(...), id, freq).await?;
  // After:
  let mut rfm = Rfm69::new(...);
  config::my_defaults(&mut rfm, id, freq).await?;
  ```
  Enables re-applying the configuration in place from a
  `Transceiver::recover` impl after a hardware fault.
- **License: relicensed from `MIT OR Apache-2.0` to `AGPL-3.0-only`.**
  Versions `0.0.1` and `0.0.2` remain available under the original
  dual license; new releases are AGPL-only. A small number of files in
  `examples/rp/` adapted from `rp-rs/rp2040-project-template` and
  `embassy-rs/embassy` keep their original `MIT OR Apache-2.0`
  licensing — see `licenses/THIRD-PARTY-NOTICES.md`. SPDX headers
  added throughout.
- **Toolchain: stable Rust, Rust 2024 edition.** Earlier versions
  required `nightly-2023-06-17` for `type_alias_impl_trait`; that
  feature is no longer used. The driver crate has no `rust-toolchain.toml`
  — it builds on whatever stable Rust the consumer has at or above the
  MSRV. The `rust-toolchain.toml` in the repo now lives under
  `examples/rp/` only (pinned to 1.95.0 for embassy 0.10
  reproducibility).
- **MSRV: 1.87.** Set by `heapless = "0.9"`. Declared via
  `rust-version` in `Cargo.toml`.
- **Dependencies updated to crates.io stable releases:**
  - `embedded-hal` 0.2 → **1.0**
  - `embedded-hal-async` 0.2 → **1.0**
  - `embassy-time` (git/main) → **0.5** (optional, behind `embassy`)
  - `embassy-sync` (git/main) → **0.8** (optional)
  - `embassy-futures` (git/main) → **0.1** (optional)
  - `heapless` 0.7 → **0.9**
  - The `[patch.crates-io]` block pointing at embassy `main` is gone;
    everything resolves to crates.io.
- **Example crate (`examples/rp/`)** updated to the embassy 0.10 stack
  (`embassy-rp 0.10`, `embassy-executor 0.10`, etc) and reorganized to
  showcase the new Stack/Runner API.

### Removed

- **The `mac` module's prior public API.** The retry/timeout/ACK MAC
  is now an implementation detail of `Runner`, no longer a separate
  user-facing module. Migration: replace direct `mac::*` calls with
  `Stack::send(dst, Flags::Ack(retries), data).await`.
- **Nightly-only `type_alias_impl_trait` use.** The driver compiles on
  stable.
- **`rust-toolchain.toml` at the repo root.** Moved under
  `examples/rp/` so library consumers aren't forced into the example
  crate's toolchain pin.

### Fixed

- Various register-sequence and bitmask issues are now regression-locked
  by the new SPI-level test pass; the previous test surface didn't
  exercise the SPI bus at all.

## [0.0.2] - 2023-06-17

### Fixed

- ACK delay before sending a MAC ACK (let the original sender
  transition back to RX before we transmit).
- MAC retry counter off-by-one.
- Invalid packet data length on the receive path.
- Various flag-decoding cleanups.

### Changed

- Config: set impedance to the common 50 Ω.
- Config: drop redundant `FreqSyn` mode transition.

## [0.0.1] - 2023-04-21

Initial release.

### Added

- `Rfm69<SPI, RESET, DIO0, DELAY>` async driver over `embedded-hal-async`
  0.2 traits.
- `Packet` wire codec (header + variable-length payload, max 61 bytes).
- A first-cut MAC layer (retry / ACK / timeout) in a separate `mac`
  module.
- RP2040 example crate (`examples/rp/`) with `rfm69`, `echo_client`,
  `echo_server` binaries.

[Unreleased]: https://github.com/chrta/rfm69-async/compare/v0.0.2...HEAD
[0.0.2]: https://github.com/chrta/rfm69-async/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/chrta/rfm69-async/releases/tag/v0.0.1
