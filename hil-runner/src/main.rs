// SPDX-License-Identifier: AGPL-3.0-only

//! Two-board hardware-in-the-loop test runner.
//!
//! Opens two serial ports (board A + board B), tees each line of output to
//! stdout with `[A]` / `[B]` prefixes, and asserts each side emits a line
//! matching its `--expect-pass-*` regex within the timeout.
//!
//! Exit codes:
//!  * `0` — both boards passed within the timeout.
//!  * `1` — either board emitted `HIL: FAIL` (immediate exit).
//!  * `2` — timeout elapsed before both passes (or the reader threads exited
//!    before producing a pass).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use regex::Regex;

#[derive(Parser, Debug)]
#[command(version, about = "Two-board RFM69 HIL test runner")]
struct Args {
    /// Serial port for board A (e.g. /dev/serial/by-id/usb-...-if00 or
    /// /dev/ttyACM0).
    #[arg(long)]
    port_a: PathBuf,

    /// Serial port for board B.
    #[arg(long)]
    port_b: PathBuf,

    /// Regex that, when matched on board A's output, marks A as passed.
    #[arg(long)]
    expect_pass_a: String,

    /// Regex that, when matched on board B's output, marks B as passed.
    #[arg(long)]
    expect_pass_b: String,

    /// Overall timeout in seconds. The runner exits 2 if both boards haven't
    /// matched their pass regex by this point.
    #[arg(long, default_value_t = 60)]
    timeout: u64,

    /// Serial baud rate. The example bins log via embassy-usb-logger which
    /// is rate-agnostic; 115200 matches the README's `screen` invocations.
    #[arg(long, default_value_t = 115200)]
    baud: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Board {
    A,
    B,
}

impl Board {
    fn label(&self) -> &'static str {
        match self {
            Board::A => "A",
            Board::B => "B",
        }
    }
}

struct Line {
    board: Board,
    text: String,
}

fn main() {
    let args = Args::parse();

    let pass_a = Regex::new(&args.expect_pass_a).expect("invalid --expect-pass-a regex");
    let pass_b = Regex::new(&args.expect_pass_b).expect("invalid --expect-pass-b regex");
    // Both bins follow the convention "emit HIL: FAIL ... when the scenario
    // fails locally"; we treat any occurrence as a hard fail across both
    // boards.
    let fail = Regex::new(r"HIL: FAIL").unwrap();

    let (tx, rx) = mpsc::channel::<Line>();
    spawn_reader(Board::A, args.port_a.clone(), args.baud, tx.clone());
    spawn_reader(Board::B, args.port_b.clone(), args.baud, tx);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(
        out,
        "[runner] streaming {} / {} (timeout {} s)",
        args.port_a.display(),
        args.port_b.display(),
        args.timeout
    );

    let deadline = Instant::now() + Duration::from_secs(args.timeout);
    let mut passed_a = false;
    let mut passed_b = false;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = writeln!(
                out,
                "[runner] TIMEOUT after {} s (A passed: {}, B passed: {})",
                args.timeout, passed_a, passed_b
            );
            std::process::exit(2);
        }
        let line = match rx.recv_timeout(remaining) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = writeln!(
                    out,
                    "[runner] both readers exited before pass (A passed: {}, B passed: {})",
                    passed_a, passed_b
                );
                std::process::exit(2);
            }
        };
        let _ = writeln!(out, "[{}] {}", line.board.label(), line.text);

        if fail.is_match(&line.text) {
            let _ = writeln!(out, "[runner] explicit FAIL on board {}", line.board.label());
            std::process::exit(1);
        }

        match line.board {
            Board::A if !passed_a && pass_a.is_match(&line.text) => {
                passed_a = true;
                let _ = writeln!(out, "[runner] board A PASS");
            }
            Board::B if !passed_b && pass_b.is_match(&line.text) => {
                passed_b = true;
                let _ = writeln!(out, "[runner] board B PASS");
            }
            _ => {}
        }

        if passed_a && passed_b {
            let _ = writeln!(out, "[runner] both boards PASS");
            std::process::exit(0);
        }
    }
}

fn spawn_reader(board: Board, path: PathBuf, baud: u32, tx: mpsc::Sender<Line>) {
    let path_string = path.to_string_lossy().into_owned();
    // 100 ms per-read timeout so the reader thread can wake periodically; if
    // the host briefly pauses on the channel send, we don't lose bytes.
    let port = serialport::new(&path_string, baud)
        .timeout(Duration::from_millis(100))
        .open()
        .unwrap_or_else(|e| panic!("failed to open {}: {}", path_string, e));

    thread::Builder::new()
        .name(format!("hil-reader-{}", board.label()))
        .spawn(move || {
            let mut reader = BufReader::new(port);
            // Bytes accumulate across reads in case a line spans multiple
            // read calls (or a TimedOut interrupts mid-line).
            let mut buf: Vec<u8> = Vec::new();
            loop {
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) => return,
                    Ok(_) => {
                        if buf.last() != Some(&b'\n') {
                            // Partial line returned without trailing \n
                            // (rare path — keep accumulating).
                            continue;
                        }
                        let text = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                        buf.clear();
                        if tx.send(Line { board, text }).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_) => return,
                }
            }
        })
        .expect("spawn failed");
}
