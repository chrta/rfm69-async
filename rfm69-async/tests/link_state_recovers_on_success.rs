// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pace_time, pkt, run_test};
use futures::FutureExt;
use rfm69_async::{Address, Flags, LinkState, TrxError};

#[test]
fn link_flips_back_up_on_first_post_down_success() {
    run_test(async |stack, trx| {
        // Drive the link Down with a streak of recv errors.
        for _ in 0..3 {
            trx.inject_err(TrxError::Spi);
        }
        {
            let wait = stack.wait_link_down().fuse();
            let pacer = pace_time(50, 1).fuse();
            futures::pin_mut!(wait);
            futures::pin_mut!(pacer);
            futures::select! {
                _ = wait => {}
                _ = pacer => panic!("wait_link_down never resolved"),
            }
        }
        assert!(matches!(stack.link_state(), LinkState::Down));

        // A single successful recv should flip it back.
        trx.inject(pkt(Address::Unicast(2), Address::Unicast(1), Flags::None, &[0x42]));
        {
            let wait = stack.wait_link_up().fuse();
            let pacer = pace_time(50, 1).fuse();
            futures::pin_mut!(wait);
            futures::pin_mut!(pacer);
            futures::select! {
                _ = wait => {}
                _ = pacer => panic!("wait_link_up never resolved"),
            }
        }
        assert!(matches!(stack.link_state(), LinkState::Up));
    });
}
