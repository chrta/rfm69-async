# rfm69-async development tasks. Run `just --list` for available recipes.

set dotenv-load

default:
    @just --list

# === HIL (two-board hardware-in-the-loop) ============================
#
# Prerequisites:
#  * Two RP2040 boards wired identically (see examples/rp/), each
#    USB-connected to the host.
#  * `picotool` installed — see upstream:
#      https://github.com/raspberrypi/picotool
#  * `just` installed — `cargo install just` works.
#  * A local `.env` (gitignored) at the repo root with four entries:
#
#       HIL_BOARD_A=E660C0D1B3818C32      # picotool serial — client board
#       HIL_BOARD_B=E660C0D1B381AB12      # picotool serial — server board
#       HIL_PORT_A=/dev/serial/by-id/usb-...
#       HIL_PORT_B=/dev/serial/by-id/usb-...
#
# Run `just hil-detect` with both boards attached to discover the
# picotool serials; the serial-port paths come from
# `ls /dev/serial/by-id/` after the bins are flashed and running.

# Print currently-attached Pico devices and their serials.
hil-detect:
    picotool info -a -d

# Build both HIL test bins (release).
hil-build:
    cd examples/rp && cargo build --release --bin hil_send_client --bin hil_recv_server

# Build the host-side runner (release).
hil-runner-build:
    cd hil-runner && cargo build --release

# Flash board A (the client) with hil_send_client.
hil-flash-a: hil-build
    picotool reboot -f -u -s "$HIL_BOARD_A"
    sleep 2
    picotool load -x -s "$HIL_BOARD_A" examples/rp/target/thumbv6m-none-eabi/release/hil_send_client

# Flash board B (the server) with hil_recv_server.
hil-flash-b: hil-build
    picotool reboot -f -u -s "$HIL_BOARD_B"
    sleep 2
    picotool load -x -s "$HIL_BOARD_B" examples/rp/target/thumbv6m-none-eabi/release/hil_recv_server

# Server flashed first so it's listening when the client starts sending;
# the client also has an 8 s pre-send pause as belt-and-suspenders.
# Build, flash both boards, then assert each emits its expected pass line.
hil-test: hil-flash-b hil-flash-a hil-runner-build
    sleep 6
    hil-runner/target/release/hil-runner \
        --port-a "$HIL_PORT_A" \
        --port-b "$HIL_PORT_B" \
        --expect-pass-a 'HIL: PASS sent=100 ack_timeouts=0' \
        --expect-pass-b 'HIL: PASS received=100' \
        --timeout 60
