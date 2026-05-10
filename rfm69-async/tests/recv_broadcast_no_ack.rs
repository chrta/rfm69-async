// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test};
use rfm69_async::{Address, Flags};

#[test]
fn recv_broadcast_with_ack_request_does_not_trigger_ack_reply() {
    run_test(async |stack, trx| {
        // Broadcast that requested an ACK -- we should still deliver but NOT
        // reply, since broadcasts don't ACK. handle_rx skips the ACK branch
        // entirely, so try_deliver runs immediately and recv resolves
        // without needing the clock to advance.
        trx.inject(pkt(Address::Unicast(7), Address::Broadcast, Flags::Ack(3), &[0xCC]));

        let p = stack.recv().await;
        assert_eq!(p.dst, Address::Broadcast);
        assert!(trx.sent().is_empty(), "broadcast should not provoke an ACK reply");
    });
}
