// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pace_time, run_test};
use futures::FutureExt;
use rfm69_async::{Address, Flags, TxError};

#[test]
fn send_ack_returns_timeout_after_exhausting_retries() {
    run_test(async |stack, trx| {
        // No ACK injected. With retries=2 and ack_timeout=50ms the runner
        // sends, waits 50ms, sends again, waits 50ms, then returns
        // AckTimeout. Pace the mock clock alongside the send future so the
        // Timer::after calls inside `with_timeout` resolve.
        let send = stack.send(Address::Unicast(2), Flags::Ack(2), &[0xCD]).fuse();
        let pacer = pace_time(60, 20).fuse(); // 1.2 s of virtual time, plenty for 2x50ms
        futures::pin_mut!(send);
        futures::pin_mut!(pacer);
        let res = futures::select! {
            r = send => r,
            _ = pacer => panic!("send didn't terminate within virtual budget"),
        };
        assert!(matches!(res, Err(TxError::AckTimeout)));
        // Two TX attempts, no ACK.
        assert_eq!(trx.sent().len(), 2);
    });
}
