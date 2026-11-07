//! The message verb's cap, as each surface spells it.

use afd_core::error_code;
use afd_wire::message_verb::MESSAGES_PER_RUN_MAX;

use crate::error::DETAIL_MESSAGE_LIMIT;

/// The cap a runner is told is the cap the count enforces: `afd_core` cannot
/// see the wire constant, so its problem hint is pinned here, beside the
/// refusal's own sentence.
#[test]
fn the_message_cap_a_runner_is_told_is_the_cap_enforced() {
    let cap = MESSAGES_PER_RUN_MAX.to_string();
    let hint = afd_core::problem::Problem::of(error_code::MESSAGE_LIMIT_REACHED).hint();
    assert!(hint.contains(&cap), "{hint}");
    assert!(
        DETAIL_MESSAGE_LIMIT.contains(&cap),
        "{DETAIL_MESSAGE_LIMIT}"
    );
}
