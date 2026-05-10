// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::run_test;
use rfm69_async::{Address, Flags};

#[test]
fn send_ack_zero_does_not_wait_for_reply() {
    // Ack(0) on the wire means "this IS the ACK reply"; sender doesn't wait.
    // Behaviourally identical to None for the Runner's send path.
    run_test(async |stack, trx| {
        let res = stack.send(Address::Unicast(2), Flags::Ack(0), &[]).await;
        assert!(matches!(res, Ok(())));
        assert_eq!(trx.sent().len(), 1);
    });
}
