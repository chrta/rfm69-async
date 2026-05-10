# RFM69-Async

RFM69-Async is an async driver for the SubGhz transceiver RFM69.

[![crates.io page](https://img.shields.io/crates/v/rfm69-async.svg)](https://crates.io/crates/rfm69-async)
[![docs.rs page](https://docs.rs/rfm69-async/badge.svg)](https://docs.rs/rfm69-async)

## Examples

Examples are found in the `examples/` folder separated by the chip manufacturer they are designed to run on. For example:

*   `examples/rp` are for the RP2040 chip.

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
