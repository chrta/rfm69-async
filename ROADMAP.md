# Roadmap

Work that is still pending on `rfm69-async`. T-shirt effort sizes:

- **XS** — < 1 h
- **S** — 1–3 h
- **M** — half a day to a day
- **L** — 1–3 days
- **XL** — multi-week effort

---

## Driver quality

**Wider register coverage.** *(L)*
`read_all_regs` exists but most setters are missing (e.g. PA, OCP, AFC, encryption / AES key, listen mode, temperature sensor). Add as user requests come in.

**SPI-level mocking via `embedded-hal-mock`.** *(L)*
Stack-level integration tests already exist (driven by a `MockTrx` that implements the `Transceiver` trait). A separate, lower-level pass would mock the SPI bus directly so `Rfm69::send` / `recv` register sequences are regression-covered. Worth picking up if a Stack-level bug points at SPI register sequences as the cause.

---

## Hardware verification

**Two-board round-trip on RP2040.** *(M)*
Single-board flashes work; SPI bus, CS, reset GPIO, `config::my_defaults`, DIO0 input + `wait_for_high` on `PacketSent`, FIFO write, and the Runner-internal MAC retry/timeout machinery all run. Still pending: pair two boards so the RX path (real `PayloadReady` wakeup, FIFO read, RSSI capture, ACK ping-pong closing the echo loop) gets exercised. Use `concurrent_demo` as the load test for the Runner state machine.

Blocked on hardware availability.

---

## Stack/Runner: deferred design extensions

These were considered during the `(Stack, Runner)` design and intentionally deferred. Revisit if a concrete user need surfaces.

- **Pubsub fan-out.** Today's rx is a `Channel`, so each packet goes to exactly one consumer. If multiple tasks want to see every packet, swap in `embassy_sync::pubsub::PubSubChannel`.
- **Per-address sockets.** `Stack::recv()` returns *every* packet addressed to us. For larger systems with multiplexed protocols on top, a Bind-by-port "socket" abstraction would help. Premature without callers.
- **Runner errors.** `Runner::run` returns `!`. Hardware errors don't yet have an escape hatch; the Runner loops forever logging. Acceptable for a single-radio driver; revisit if a fault-recovery story emerges.
- **Send-bounded futures on `Transceiver`.** The trait-level `#[allow(async_fn_in_trait)]` stays. If a future user needs `Send`-bounded radio futures (cross-executor scenarios), they can desugar a wrapper trait that adds the bound; not worth complicating the public trait speculatively.

---

## Multi-MCU support

The driver itself is already MCU-agnostic — it depends only on `embedded-hal`/`-async` traits and (for the Stack runner) `embassy-time`. The work here is mostly about examples, CI, and making sure the small embassy coupling doesn't bind us to one HAL.

- **Reorganize the examples directory.** *(M)*
  Today: `examples/rp/`. Proposed: keep that path but add siblings (`examples/stm32/`, `examples/nrf/`, …), each a self-contained binary crate with its own `Cargo.toml`, `memory.x`, and `build.rs`. Keep the "two-crate, non-workspace" layout — it has served well — but add a top-level `Cargo.toml` workspace **only** if it doesn't force shared dep versions across HALs (an `exclude = […]` workspace can work).
- **Add an STM32 example.** *(L)*
  Suggested target: an STM32WL or any STM32F4/L4 dev board with SPI broken out. Mirror the structure of `examples/rp/`. The proof point that "multi-MCU" actually works without driver changes.
- **Add an nRF example.** *(L)*
  nRF52840 is the easiest second target. Adafruit Feather nRF52840 + RFM69 modules pair well.
- **Optional: shared example helper crate.** *(M)*
  If the bins end up duplicated 1:1 across HAL examples, factor the common protocol logic into a tiny `examples/common/` crate that takes the constructed `Stack<…>` as input. Defer until the duplication is real — premature abstraction across HALs is worse than copy-paste.
- **CI matrix.** *(S)*
  `.github/workflows/rust.yml` currently builds only `examples/rp`. Extend to a matrix over each example crate so every supported HAL gets compile coverage on every PR.

**Effort: ~1 week, dominated by setting up two new HAL examples and verifying on hardware.**

---

## Release polish

- **Crate-level rustdoc.** *(S)*
  `lib.rs` is currently bare. Add module-level docs explaining the Stack/Runner model, the `Transceiver` trait, the optional DIO0 pin, and the `embassy` / `defmt` / `log` features.
- **Cut `0.1.0`.** *(XS)*
  Bump from `0.0.x` to `0.1.0` to signal "usable, semver-tracked." Update `examples/*/Cargo.toml` paths/versions accordingly.
- **Add `CHANGELOG.md`.** *(XS)*
  Keep-a-changelog format. The 0.0.1 → 0.1.0 release is a good first entry — call out the relicense and the Stack rewrite as the headlines.
