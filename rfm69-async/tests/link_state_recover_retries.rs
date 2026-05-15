// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pace_time, run_test};
use futures::FutureExt;
use rfm69_async::{LinkState, TrxError};

#[test]
fn recover_retries_after_failure_until_success() {
    run_test(async |stack, trx| {
        // Drive the link Down.
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

        // Two failed recover attempts then one success. The Runner sleeps
        // `recover_backoff` (500 ms in run_test) between attempts; pace
        // time forward far enough to clear at least two backoffs.
        trx.inject_recover_err(TrxError::Reset);
        trx.inject_recover_err(TrxError::Reset);
        trx.inject_recover_ok();

        {
            let wait = stack.wait_link_up().fuse();
            // 3 recover calls × 500 ms backoff + slack.
            let pacer = pace_time(2_000, 1).fuse();
            futures::pin_mut!(wait);
            futures::pin_mut!(pacer);
            futures::select! {
                _ = wait => {}
                _ = pacer => panic!("wait_link_up never resolved"),
            }
        }

        assert!(matches!(stack.link_state(), LinkState::Up));
        assert!(
            trx.recover_calls() >= 3,
            "expected at least 3 recover calls, got {}",
            trx.recover_calls()
        );
    });
}
