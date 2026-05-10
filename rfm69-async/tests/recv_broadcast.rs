// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test};
use rfm69_async::{Address, Flags};

#[test]
fn recv_returns_broadcast() {
    run_test(async |stack, trx| {
        trx.inject(pkt(Address::Unicast(7), Address::Broadcast, Flags::None, &[0xBB]));
        let p = stack.recv().await;
        assert_eq!(p.dst, Address::Broadcast);
        assert_eq!(p.data.as_slice(), &[0xBB]);
    });
}
