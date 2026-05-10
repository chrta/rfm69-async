// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test};
use rfm69_async::{Address, Flags};

#[test]
fn recv_returns_unicast_addressed_to_us() {
    run_test(async |stack, trx| {
        trx.inject(pkt(
            Address::Unicast(7),
            Address::Unicast(1),
            Flags::None,
            &[0x10, 0x20],
        ));

        let p = stack.recv().await;
        assert_eq!(p.src, Address::Unicast(7));
        assert_eq!(p.dst, Address::Unicast(1));
        assert_eq!(p.data.as_slice(), &[0x10, 0x20]);
    });
}
