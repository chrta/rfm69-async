// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::run_test;
use rfm69_async::{Address, Flags};

#[test]
fn send_no_ack_reaches_mock_with_correct_header() {
    run_test(async |stack, trx| {
        let res = stack.send(Address::Unicast(2), Flags::None, &[0xAA, 0xBB]).await;
        assert!(matches!(res, Ok(())));

        let sent = trx.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].src, Address::Unicast(1));
        assert_eq!(sent[0].dst, Address::Unicast(2));
        assert!(matches!(sent[0].flags, Flags::None));
        assert_eq!(sent[0].data.as_slice(), &[0xAA, 0xBB]);
    });
}
