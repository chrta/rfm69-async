# RFM69-Async

RFM69-Async is an async driver for the SubGhz transceiver RFM69.

[![crates.io page](https://img.shields.io/crates/v/rfm69-async.svg)](https://crates.io/crates/rfm69-async)
[![docs.rs page](https://docs.rs/rfm69-async/badge.svg)](https://docs.rs/rfm69-async)

## Examples

Examples are found in the `examples/` folder separated by the chip manufacturer they are designed to run on. For example:

*   `examples/rp` are for the RP2040 chip.

The RP2040 binaries are:

*   `blinky` — board sanity check; no radio involved.
*   `rfm69` — single-task send/receive loop.
*   `echo_client` and `echo_server` — paired roles for round-trip testing
    over two boards: client sends, server echoes back.
*   `concurrent_demo` — splits send and receive across independent embassy
    tasks driven by a single radio Runner. Headline feature of the
    `Stack` / `Runner` API: user tasks call `stack.send()` and
    `stack.recv()` concurrently with no manual rx/tx interleaving.

### Running examples

- Install tools to debug/flash the firmware. For example to flash the firmware to the rpi pico via USB:

```bash
cargo install elf2uf2-rs
```

- Change directory to the sample's base directory. For example:

```bash
cd examples/rp
```

- Build the example

For example:

```bash
cargo build --bin rfm69 --release
```

- Put the Pico in BOOTSEL mode

Hold the **BOOTSEL** button while plugging the USB cable in (or, if it is already
plugged in, hold **BOOTSEL** and tap **RESET**). The board enumerates as a USB
mass-storage device named `RPI-RP2`.

- Ensure the `RPI-RP2` volume is mounted

`elf2uf2-rs -d` looks at the mount table to find the Pico, so simply having the
device appear in `dmesg` / `lsblk` is not enough — it must actually be mounted.
On desktops with auto-mount this happens automatically; on minimal setups mount
it manually, e.g.:

```bash
udisksctl mount -b /dev/sda1     # adjust device per `lsblk`
```

- Flash the example

For example:

```bash
elf2uf2-rs -d target/thumbv6m-none-eabi/release/rfm69
```

### Observing the output on a connected PC

The bins under `examples/rp/` ship two independent log transports. Pick whichever
matches your hardware setup:

#### USB CDC serial (no debug probe required)

The default. After the Pico finishes booting it enumerates a second time as a
USB CDC ACM device (`/dev/ttyACM*` on Linux, `/dev/tty.usbmodem*` on macOS,
`COM*` on Windows) driven by `embassy-usb-logger`. All `log::*` output from the
example bins **and from the driver crate** is forwarded over this serial link.
Read it with any terminal:

```bash
# Linux
screen /dev/ttyACM0 115200          # exit with Ctrl-A Ctrl-\
# or
picocom -b 115200 /dev/ttyACM0
```

The first line you should see after a freshly flashed `echo_server` is
`--- Staring echo server ---`, followed by `Reading version register...` /
`Version: 0x24` from the driver. Note that the bins wait a few seconds at boot
so the host has time to finish enumerating the USB device before logging starts.

This transport carries the driver's logs because the examples crate enables
the driver's `log` cargo feature. If you depend on `rfm69-async` in your own
project and want the same behaviour, enable the `log` feature on the driver
dependency:

```toml
rfm69-async = { version = "…", features = ["embassy", "log"] }
```

#### defmt-rtt over an SWD debug probe (optional)

The bins also pull in `defmt-rtt` and `panic-probe`. If you have a debug probe
wired to the Pico's SWD pins (e.g. the [Raspberry Pi Debug Probe], or a second
Pico flashed with the `debugprobe` firmware), you can flash and stream
defmt-formatted output directly with `probe-rs` instead of UF2:

```bash
cargo install probe-rs-tools
probe-rs run --chip RP2040 target/thumbv6m-none-eabi/release/rfm69
```

`probe-rs run` halts the target on a panic and prints the decoded panic
message; UF2 flashing has no equivalent. To also forward driver-internal logs
through this transport, additionally enable the driver's `defmt` feature
(already on in the examples).

[Raspberry Pi Debug Probe]: https://www.raspberrypi.com/products/debug-probe/

## Changelog

User-visible changes are tracked in [`rfm69-async/CHANGELOG.md`](rfm69-async/CHANGELOG.md),
following the [Keep a Changelog](https://keepachangelog.com/) format.

### Releasing

Version bumps are driven by [`cargo-release`](https://github.com/crate-ci/cargo-release);
configuration lives in [`rfm69-async/release.toml`](rfm69-async/release.toml).
Edit the `## [Unreleased]` section of the changelog with the user-facing
notes for the new release, then:

```bash
cargo install cargo-release          # one-time
cd rfm69-async
cargo release 0.1.0                  # dry-run preview
cargo release 0.1.0 --execute        # bump Cargo.toml + examples path-dep,
                                     # rename the changelog heading, commit, tag
git push --follow-tags               # explicit — release.toml has push = false
cargo publish                        # explicit — release.toml has publish = false
```

## License

This work is licensed under the GNU Affero General Public License v3.0 only
([LICENSE](LICENSE) or <https://www.gnu.org/licenses/agpl-3.0.html>).

`SPDX-License-Identifier: AGPL-3.0-only`

A small number of files in `examples/rp/` were copied or adapted from the
`rp-rs/rp2040-project-template` and `embassy-rs/embassy` projects and remain
under their original `MIT OR Apache-2.0` licensing; see
[`licenses/THIRD-PARTY-NOTICES.md`](licenses/THIRD-PARTY-NOTICES.md) for
details. Each file's effective license is declared in its own
`SPDX-License-Identifier` header.

Versions `0.0.1` and `0.0.2` were released under the dual `MIT OR Apache-2.0`
license; that licensing remains in effect for those published versions.

### Contribution

Any contribution intentionally submitted for inclusion in the work by you
shall be licensed under AGPL-3.0-only, without any additional terms or
conditions.

### Credits

The code is inspired by https://github.com/almusil/rfm69, which was
inspired by older https://github.com/lolzballs/rfm69.
