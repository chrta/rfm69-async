// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test};
use rfm69_async::{Address, Flags};

#[test]
fn send_ack_completes_when_ack_arrives() {
    run_test(async |stack, trx| {
        // Inject the ACK before calling send. The Runner pulls the user's
        // request from the channel, sends it, then awaits the ACK; by then
        // our injected ACK is sitting in the inbox waiting.
        trx.inject(pkt(
            Address::Unicast(2),
            Address::Unicast(1),
            Flags::Ack(0), // ACK reply
            &[],
        ));

        let res = stack.send(Address::Unicast(2), Flags::Ack(2), &[0x42]).await;
        assert!(matches!(res, Ok(())));
        // Exactly one TX (no retries).
        assert_eq!(trx.sent().len(), 1);
    });
}
