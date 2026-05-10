// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::{pkt, run_test, yield_now};
use rfm69_async::{Address, Flags, LinkState, TrxError};

#[test]
fn two_errors_followed_by_success_keep_link_up() {
    run_test(async |stack, trx| {
        // LINK_DOWN_STREAK == 3. Two errors in a row, then a real packet —
        // the success resets the streak before it crosses the threshold,
        // so state must never have flipped to Down.
        trx.inject_err(TrxError::Spi);
        trx.inject_err(TrxError::Spi);
        trx.inject(pkt(Address::Unicast(2), Address::Unicast(1), Flags::None, &[0x42]));

        // Drain the user-side packet to confirm the runner processed all
        // three injected items.
        let _ = stack.recv().await;

        // Give the runner a few more yields for any followup work to settle.
        for _ in 0..5 {
            yield_now().await;
        }
        assert!(stack.is_link_up());
        assert!(matches!(stack.link_state(), LinkState::Up));
    });
}
