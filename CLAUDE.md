# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

This is a two-crate Cargo setup, NOT a workspace:

- `rfm69-async/` — the publishable `no_std` driver crate (the library).
- `examples/rp/` — a separate, self-contained binary crate of RP2040 examples that depends on the driver via a local `path = "../../rfm69-async"`.

Each directory has its own `Cargo.toml` and `Cargo.lock`. Cargo commands must be run from the appropriate subdirectory; there is no top-level `Cargo.toml`.

Toolchain layout:
- The driver crate (`rfm69-async/`) declares its MSRV via `rust-version = "1.88"` in `Cargo.toml`. The base crate builds on 1.87 (the floor `heapless = "0.9"` sets; the rest of our deps are below it), but the `embassy` feature uses a let-chain in `stack.rs` that needs 1.88, so the package-level floor is 1.88. CI's `msrv` job runs `cargo msrv verify` (default + `--all-features`) to keep this honest — `verify` reads the floor from `Cargo.toml`'s `rust-version`, so there's no second copy of the version to drift out of sync. The driver has no `rust-toolchain.toml` and no target requirement — it builds on whatever stable users have, host or cross.
- The examples crate (`examples/rp/`) carries its own `rust-toolchain.toml` pinned to `1.95.0` and the `thumbv6m-none-eabi` target. Pinning is for hardware-build reproducibility against the embassy 0.10 stack; bump in lockstep with embassy-rp / `fixed` releases when needed. Lowest known-good is 1.93 (transitive `fixed = 1.31` requires it).

The driver compiles cleanly on stable Rust — no `#![feature(...)]` gates anywhere. Earlier history used `nightly-2023-06-17` + `type_alias_impl_trait`; that's gone.

The driver uses **`embedded-hal = "1"` and `embedded-hal-async = "1"`** (1.0 stable), plus `heapless = "0.9"`. The optional `embassy-time = "0.5"` is gated behind the `embassy` feature. The examples crate is on the released embassy 0.10 stack (`embassy-rp 0.10`, `embassy-executor 0.10`, etc.) — there is **no** `[patch.crates-io]` block; everything resolves to crates.io releases.

The driver crate ships three cargo features (no defaults — callers opt in):
- `embassy` — pulls in `embassy-time` and enables the `mac` module (timeout/retry logic uses `with_timeout`).
- `log` — pulls in `log = "0.4"` and routes the driver's internal logging macros through it. Required to see driver-internal logs over the embassy USB CDC logger workflow used by the examples.
- `defmt` — adds `defmt::Format` derives on `Address`, `Flags`, `Packet`, `Error`, pulls in `defmt = "1"` plus `heapless/defmt`, and routes the driver's internal logging macros through `defmt::*`. Required for the defmt-rtt / probe-rs workflow.

The internal logging is implemented in `src/fmt.rs` (`info!` / `debug!` / `warn!` / `error!`). Call sites in the driver use the bare names — never `log::info!` directly. With both `log` and `defmt` enabled, both backends fire (mirrors the embassy-net pattern); with neither enabled the macros expand to a no-op that still consumes its arguments to avoid `unused_variables` warnings.

The examples crate enables all three: `rfm69-async = { ..., features = ["embassy", "defmt", "log"] }` — `log` for the USB-CDC serial workflow, `defmt` for the optional probe-rs / SWD workflow.

## Common commands

CI (`.github/workflows/rust.yml`) runs the following on each crate; mirror these locally before pushing:

```bash
# Driver crate
cd rfm69-async
cargo build
cargo clippy
cargo fmt --check
cargo test            # host tests, no target flag
cargo test --features embassy   # host tests + Stack/Runner integration bins
cargo doc
cargo build --features defmt   # also exercise the defmt-gated derives
cargo build --features embassy # exercise the optional MAC layer
cargo +1.88 build --all-features  # MSRV gate (embassy's let-chain needs 1.88)
# CI runs this as `cargo msrv verify` (reads rust-version from Cargo.toml).
# Locally that needs `cargo install cargo-msrv`; the +1.88 build above is the
# zero-install equivalent. To mirror CI exactly:
#   cargo msrv verify && cargo msrv verify -- cargo check --all-features

# Examples (RP2040; the thumbv6m-none-eabi target is in rust-toolchain.toml)
cd examples/rp
cargo build
cargo clippy
cargo fmt --check
cargo build --release --bin rfm69   # the README's flashing path
```

Build a single example binary and flash to a Pico in BOOTSEL mode:

```bash
cd examples/rp
cargo build --bin rfm69 --release
elf2uf2-rs -d target/thumbv6m-none-eabi/release/rfm69
```

Available example bins live in `examples/rp/src/bin/`: `rfm69`, `echo_client`, `echo_server`, `blinky`, `concurrent_demo`. The first three drive the radio from `main`'s task using `embassy_futures::join`. `concurrent_demo` instead spawns the runner / rx / tx as three independent embassy tasks — that's the canonical pattern when something needs `Stack<'static>` to be moved across task boundaries, and the worked example for users porting their own apps to the new API.

`rustfmt.toml` only sets `max_width = 120`. The previously-configured `group_imports` and `imports_granularity` were nightly-only and got dropped along with the toolchain bump. The VS Code config sets `rust-analyzer.cargo.target = "thumbv6m-none-eabi"` and points `linkedProjects` at `examples/rp/Cargo.toml` by default.

## Commit messages

- **One topic per commit.** If you touched two unrelated things, split. Prefer scope-prefixed subjects (`rfm69-async: ...`, `examples: rp: ...`) when the change is crate-local, or a plain imperative subject otherwise. Subject ≤ ~70 chars.
- **Each commit must build clean.** Before committing, run `cargo fmt --check` and `cargo clippy` on every crate the commit touches. CI gates on both per crate; a broken commit in the middle of a series breaks `git bisect`.
- **Be terse.** Body is ~3–10 short lines. Lead with *why*, mention *what* only when it isn't obvious from the diff, then stop. Don't enumerate every file you touched, every test you ran, or every flag you passed — the diff and CI cover that. Skip mechanical fallout (rustfmt-driven import reordering, lockfile bumps, "verified the existing tests pass"). If the body is creeping past 15 lines, the commit is doing too much, or you're over-explaining a small change.
- **Don't reference ROADMAP phases / items / step numbers** in commit messages or in any non-`ROADMAP.md` file. Those references rot when the document is reorganized; the diff itself describes the change.
- **Trailers.** End every commit with both, in this order:
  ```
  Assisted-by: Claude:claude-opus-4-7
  Signed-off-by: Your Name <you@example.com>
  ```
  Drop `Assisted-by:` for commits whose content the assistant didn't author. Write the `Signed-off-by` line into the message body explicitly — don't rely on `git commit -s`.

## Architecture

### Driver (`rfm69-async/src/`)

`lib.rs` re-exports the public surface: `Rfm69`, `Address`, `Flags`, `Packet`, `Error`, plus the `config` and `mac` modules.

The `Rfm69<SPI, RESET, DIO0, DELAY>` struct in `rfm.rs` is the low-level transceiver. It is fully generic over `embedded-hal-async` 1.0 traits (`SpiDevice`, `OutputPin`, `InputPin + Wait`, `DelayNs`) so it is not tied to any HAL. Two operating styles are supported:

- **DIO0 connected (preferred)** — `send`/`recv` await `dio0.wait_for_high()` for hardware-driven `PacketSent` / `PayloadReady` events. Before each operation, `send` writes `DioMapping1 = 0x00` (PacketSent on DIO0) and `recv` writes `0x40` (PayloadReady on DIO0).
- **DIO0 absent** — `dio0` field is `None` and the driver polls `IrqFlags2` instead. Pass `None::<SomeConcretePin>` at construction.

Internal SPI helpers (`read_register` / `write_register` / `update_register` / `write_registers` / `read_registers`) implement the RFM69's "address byte with high bit = write" protocol via `embedded-hal-async` `Operation`s. Frequency, bitrate, and FDEV setters scale by `FOSC / 2^19`; values use `F_SCALE = 1_000_000` to keep precision in integer math — preserve this convention if adding similar setters.

The cached `mode: OpMode` field MUST be updated through `set_mode`; there are paths (`reset` after version check, `send`/`recv` transitions to Standby) that depend on it.

### Transceiver trait (`traits.rs`)

`Transceiver` is a small async trait with `send(&mut self, &Packet)` and `recv(&mut self)` returning `TrxError` (a lossy enum that collapses `Error<SPI, RESET, DIO0>` for callers that hold a generic `TRX`). `Rfm69<...>` implements it via a blanket impl in `rfm.rs`. The crate-internal `From<Error<...>> for TrxError` (in `error.rs`) does the collapse.

`async fn in trait` is stable and used directly. The rustc `async_fn_in_trait` lint about unspecified Send-bound on the returned futures is allow-listed at the trait level — embassy on the targets this crate supports is single-executor, so Send isn't needed; if a future user crosses executor threads, they wrap a Send-bound subtrait. `TrxError` deliberately stays lossy (collapsing the parametric `Error<SPI, RESET, DIO0>` at the trait boundary) — see the `TrxError` rustdoc for the reasoning; don't try to thread the parametric error through `Stack` / `Runner`.

### Stack / Runner (`stack.rs`)

The smoltcp-style Stack/Runner split. Public surface:

- `StackResources<const N_RX = 4>` — caller-allocated channel buffers (`Channel<Packet, N_RX>` for rx, `Channel<TxRequest, 1>` + `Signal<Result>` + `Mutex<()>` for tx). `const fn new()` so `static StackResources` works.
- `Stack<'a>` — `Copy` user-facing handle. Methods: `send(dst, flags, data) -> Result<(), TxError>`, `recv() -> Packet`, `address() -> Address`. Cloneable; pass to multiple tasks.
- `Runner<'a, TRX: Transceiver>` — owns the radio. `run() -> !` is a `select(tx_request_recv, trx.recv)` loop in `handle_tx`/`handle_rx`. ACK retry/timeout logic lives in `handle_tx` via `MacTiming`.
- Construction: `Stack::new(trx, address, &mut resources, MacTiming::default()) -> (Stack<'a>, Runner<'a, TRX>)`.

Gated on **`feature = "embassy"`** because it uses `embassy-time` (timing), `embassy-sync` (Channel/Mutex/Signal), and `embassy-futures` (`select`). The whole module is `#[cfg(feature = "embassy")]` in `lib.rs`.

Key implementation invariants worth preserving:
- `Stack::send` must lock `tx_mutex`, *then* `signal.reset()`, *then* enqueue the request. Reset-then-send under the mutex is the cancellation-safety contract.
- `Runner::handle_rx` only ACKs when `flags == Flags::Ack(n)` with `n > 0` AND `dst == self.address` (broadcasts never ACK; ACK-with-n=0 is itself an ACK reply, no recursive ACK).
- `Runner::wait_for_ack` delivers non-ACK packets to the rx queue rather than dropping them, so the ACK race doesn't cost user packets.
- `try_deliver` uses non-blocking `try_send`; an overflowing rx queue logs a warning and drops, never backpressures the radio.

ACK semantics in `Flags` (`flags.rs`) are subtle:
- `Flags::Ack(0)` on the wire means "this IS an ACK reply" — receivers don't ACK back.
- `Flags::Ack(n)` for `n >= 1` means "I want an ACK; sender will retry up to `n` times."
- The wire encoding adds 1 (`as_u8`) and `from_u8` only decodes values 1..=4; anything else degrades to `Flags::None`.

`Stack::recv` only delivers packets addressed to the Stack's own `Address` (Unicast match) or `Address::Broadcast` (`0xFF`); other unicast packets are filtered out by the Runner before they reach the queue.

### Packet format

`Packet` (`packet.rs`) is `[len][src][dst][flags][payload...]` on the FIFO. `len` excludes itself and must be ≥ 3 (header-only packet). Max payload is 61 bytes (`MAX_PAYLOAD_DATA_LENGTH`), giving a 64-byte FIFO frame plus the length byte (65). The receive path also captures RSSI from `RegRssiValue` (read after Standby, scaled `-reg/2`) and stores it on the returned `Packet`.

### Configurations (`config.rs`)

Two preset initializers:
- `low_power_lab_defaults` — compatibility with the LowPowerLab Arduino library's defaults (FSK, 55555 bps, sync `[0x2D, network_id]`, variable packet 66, CRC). Marked "not tested/used" in source.
- `my_defaults` — GFSK BT=0.5, 100 kbps; otherwise similar.

Both consume the `Rfm69` by value and return it, so the call site pattern is `let rfm = config::my_defaults(Rfm69::new(...), network_id, freq).await?;`.

### Error type

`Error<SPI, RESET, DIO0>` is generic over the three peripheral error types. The `stack` module wraps it again as `TxError` (private) to add `AckTimeout`. The `Transceiver`/`Stack` boundary uses `TrxError` (lossy, in `traits.rs`). `Error` derives `defmt::Format` under the `defmt` feature; `Address`, `Flags`, `Packet`, and `TrxError` do too. Enabling `defmt` requires `heapless/defmt` to be enabled — the cargo `defmt = ["dep:defmt", "heapless/defmt"]` line in `Cargo.toml` does this — because `Packet::data: Vec<u8, 61>` needs the heapless side to provide the `Format` impl.

### Tests

Host unit tests live next to their modules (`address.rs`, `flags.rs`, `packet.rs`) and run on plain `cargo test` with no extra features.

Stack/Runner integration tests live in `rfm69-async/tests/*.rs` and run on `cargo test --features embassy`. Each top-level `tests/*.rs` is a separate test binary — Cargo compiles one process per file. That isolation is deliberate: `embassy_time::MockDriver` and the `generic-queue-16` timer queue are global statics that don't reset cleanly between in-process tests. Shared fixtures (the `MockTrx` Transceiver impl, the `run_test` driver, `pace_time` / `yield_now` helpers) live in `tests/common/mod.rs`; subdirectories under `tests/` aren't auto-built by Cargo, so they don't need a `[[test]]` entry. Each top-level test file does need its own `[[test]]` block with `required-features = ["embassy"]` — without it Stack / Runner aren't compiled.
