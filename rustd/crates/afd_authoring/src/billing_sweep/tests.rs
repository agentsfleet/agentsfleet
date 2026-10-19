use afd_billing::biller::Release;

use super::release;

#[test]
fn a_finished_hold_is_released_by_its_outcome() {
    let cases = [
        (Some("completed"), Release::Completed),
        (Some("failed"), Release::Failed),
        (Some("timed_out"), Release::Failed),
        (Some("cancelled"), Release::Failed),
        // The attempt row is gone, or its stored outcome is unreadable.
        (None, Release::Failed),
        (Some("not-an-outcome"), Release::Failed),
    ];
    for (outcome, expected) in cases {
        assert_eq!(release(outcome), expected, "outcome {outcome:?}");
    }
}
