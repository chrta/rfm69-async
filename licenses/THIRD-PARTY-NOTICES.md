# Third-Party Notices

The following files in this repository are derived from third-party sources
and remain under their original `MIT OR Apache-2.0` licensing. The full
license texts are in `licenses/LICENSE-MIT` and `licenses/LICENSE-Apache-2.0`.

The rest of the repository is licensed under `AGPL-3.0-only` (see top-level
`LICENSE`). Each file's actual license is declared in its own
`SPDX-License-Identifier` header.

## Files derived from `rp-rs/rp2040-project-template`

Source: <https://github.com/rp-rs/rp2040-project-template>
Copyright: `Copyright (c) 2021 rp-rs organization`
License: `MIT OR Apache-2.0`

- `examples/rp/build.rs` — verbatim copy.
- `examples/rp/memory.x` — verbatim copy of the `MEMORY { ... }` block.

## Files derived from `embassy-rs/embassy`

Source: <https://github.com/embassy-rs/embassy>
Copyright: `Copyright (c) Embassy project contributors`
License: `MIT OR Apache-2.0`

- `examples/rp/src/bin/blinky.rs` — close adaptation of `examples/rp/src/bin/blinky.rs` from upstream.
- `examples/rp/src/bin/rfm69.rs` — partial: the `bind_interrupts!`, `logger_task`, and embassy `main` setup are adapted from upstream `examples/rp/src/bin/usb_logger.rs`. The radio-specific code is original work under AGPL-3.0-only.
- `examples/rp/src/bin/echo_client.rs` — partial: same scaffolding as above.
- `examples/rp/src/bin/echo_server.rs` — partial: same scaffolding as above.

The mixed-license files use the SPDX expression
`(AGPL-3.0-only AND (MIT OR Apache-2.0))` in their per-file header to reflect
the combined sources.
