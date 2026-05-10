# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

This is a two-crate Cargo setup, NOT a workspace:

- `rfm69-async/` — the publishable `no_std` driver crate (the library).
- `examples/rp/` — a separate, self-contained binary crate of RP2040 examples that depends on the driver via a local `path = "../../rfm69-async"`.

Each directory has its own `Cargo.toml` and `Cargo.lock`. Cargo commands must be run from the appropriate subdirectory; there is no top-level `Cargo.toml`.

Toolchain layout:
- The driver crate (`rfm69-async/`) declares its MSRV via `rust-version = "1.87"` in `Cargo.toml`. The 1.87 floor is set by `heapless = "0.9"` (the rest of our deps are below 1.87). The driver has no `rust-toolchain.toml` and no target requirement — it builds on whatever stable users have, host or cross.
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
cargo doc
cargo build --features defmt   # also exercise the defmt-gated derives
cargo build --features embassy # exercise the optional MAC layer

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

Available example bins live in `examples/rp/src/bin/`: `rfm69`, `echo_client`, `echo_server`, `blinky`.

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

### MAC layer (`mac.rs`)

A thin protocol on top of `Rfm69::send`/`recv`. `send_packet` / `receive_packet` / `wait_for_mac_ack` add source/destination addressing, optional ACK with retry-count, and (for `Flags::Ack`) a `with_timeout`-bounded ACK wait + retry loop.

The MAC layer is **gated on `feature = "embassy"`** because it uses `embassy_time::{with_timeout, Timer, Duration}` for timing. The driver core has no such dependency. When editing `mac.rs`, keep all `embassy_time` usage behind `#[cfg(feature = "embassy")]`.

ACK semantics in `Flags` (`flags.rs`) are subtle:
- `Flags::Ack(0)` on the wire means "this IS an ACK reply" — receivers don't ACK back.
- `Flags::Ack(n)` for `n >= 1` means "I want an ACK; sender will retry up to `n` times."
- The wire encoding adds 1 (`as_u8`) and `from_u8` only decodes values 1..=4; anything else degrades to `Flags::None`.

`receive_packet` only returns packets addressed to `dst` (Unicast match) or `Address::Broadcast` (`0xFF`); other unicast packets are silently consumed and the loop continues.

### Packet format

`Packet` (`packet.rs`) is `[len][src][dst][flags][payload...]` on the FIFO. `len` excludes itself and must be ≥ 3 (header-only packet). Max payload is 61 bytes (`MAX_PAYLOAD_DATA_LENGTH`), giving a 64-byte FIFO frame plus the length byte (65). The receive path also captures RSSI from `RegRssiValue` (read after Standby, scaled `-reg/2`) and stores it on the returned `Packet`.

### Configurations (`config.rs`)

Two preset initializers:
- `low_power_lab_defaults` — compatibility with the LowPowerLab Arduino library's defaults (FSK, 55555 bps, sync `[0x2D, network_id]`, variable packet 66, CRC). Marked "not tested/used" in source.
- `my_defaults` — GFSK BT=0.5, 100 kbps; otherwise similar.

Both consume the `Rfm69` by value and return it, so the call site pattern is `let rfm = config::my_defaults(Rfm69::new(...), network_id, freq).await?;`.

### Error type

`Error<SPI, RESET, DIO0>` is generic over the three peripheral error types. The `mac` module wraps it again as `TxError` to add `AckTimeout`. Both derive `defmt::Format` under the (now properly declared) `defmt` feature; `Address`, `Flags`, and `Packet` do too. Enabling `defmt` requires `heapless/defmt` to be enabled — the cargo `defmt = ["dep:defmt", "heapless/defmt"]` line in `Cargo.toml` does this — because `Packet::data: Vec<u8, 61>` needs the heapless side to provide the `Format` impl.
