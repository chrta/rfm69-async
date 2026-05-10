// SPDX-License-Identifier: AGPL-3.0-only

mod common;

use common::run_test;
use rfm69_async::LinkState;

#[test]
fn fresh_stack_reports_link_up() {
    run_test(async |stack, _trx| {
        assert!(stack.is_link_up());
        assert!(matches!(stack.link_state(), LinkState::Up));
    });
}
