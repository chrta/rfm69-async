// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test};
use rfm69_async::{Address, Flags};

#[test]
fn recv_filters_unicast_for_other_address() {
    run_test(async |stack, trx| {
        // Two packets: one for 99 (drop), one for us (deliver).
        trx.inject(pkt(Address::Unicast(7), Address::Unicast(99), Flags::None, &[0xFF]));
        trx.inject(pkt(Address::Unicast(7), Address::Unicast(1), Flags::None, &[0x33]));

        let p = stack.recv().await;
        // Only the second packet reaches us; the first was filtered.
        assert_eq!(p.dst, Address::Unicast(1));
        assert_eq!(p.data.as_slice(), &[0x33]);
    });
}
