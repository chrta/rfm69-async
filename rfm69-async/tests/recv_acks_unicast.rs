// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pace_time, pkt, run_test};
use futures::FutureExt;
use rfm69_async::{Address, Flags};

#[test]
fn recv_packet_requesting_ack_triggers_ack_reply() {
    run_test(async |stack, trx| {
        // Sender wants an ACK (n>0), unicast to us. handle_rx ACKs *before*
        // try_deliver, so by the time `recv` returns, the ACK has already
        // been sent -- but only if Timer::after(ack_tx_delay) resolved,
        // which needs MockDriver to advance. Pace alongside.
        trx.inject(pkt(Address::Unicast(7), Address::Unicast(1), Flags::Ack(3), &[0xAB]));

        let recv = stack.recv().fuse();
        let pacer = pace_time(20, 1).fuse();
        futures::pin_mut!(recv);
        futures::pin_mut!(pacer);
        let p = futures::select! {
            p = recv => p,
            _ = pacer => panic!("recv didn't return"),
        };
        assert_eq!(p.src, Address::Unicast(7));

        let sent = trx.sent();
        assert_eq!(sent.len(), 1, "expected exactly one ACK reply");
        assert_eq!(sent[0].src, Address::Unicast(1));
        assert_eq!(sent[0].dst, Address::Unicast(7));
        assert!(matches!(sent[0].flags, Flags::Ack(0))); // wire byte 1 == "I am the ACK"
        assert_eq!(sent[0].data.len(), 0);
    });
}
