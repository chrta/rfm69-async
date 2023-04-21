# RFM69-Async

RFM69-Async is an async driver for the SubGhz transceiver RFM69.
The code is inspired by https://github.com/almusil/rfm69

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

- Flash the example

For example:

```bash
elf2uf2-rs -d target/thumbv6m-none-eabi/release/rfm69
```

## License

This work is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.