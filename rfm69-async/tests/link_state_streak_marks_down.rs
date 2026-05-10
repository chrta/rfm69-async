// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pace_time, run_test};
use futures::FutureExt;
use rfm69_async::{LinkState, TrxError};

#[test]
fn three_consecutive_rx_errors_flip_link_down() {
    run_test(async |stack, trx| {
        // LINK_DOWN_STREAK == 3. Inject exactly that many recv errors and
        // verify the runner flips the state.
        for _ in 0..3 {
            trx.inject_err(TrxError::Spi);
        }

        let wait = stack.wait_link_down().fuse();
        let pacer = pace_time(50, 1).fuse();
        futures::pin_mut!(wait);
        futures::pin_mut!(pacer);
        futures::select! {
            _ = wait => {}
            _ = pacer => panic!("wait_link_down never resolved"),
        }
        assert!(matches!(stack.link_state(), LinkState::Down));
        assert!(!stack.is_link_up());
    });
}
