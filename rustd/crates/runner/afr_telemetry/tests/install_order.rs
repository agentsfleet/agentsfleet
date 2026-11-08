//! A recorder takes in the provider's slot and its own, or in neither.
//!
//! The slots are process-wide and take once, so this suite holds one test and
//! owns its binary.
#![cfg(feature = "test-util")]

use afr_telemetry::labels::PushFailure;
use afr_telemetry::record;
use afr_telemetry::testing::Tally;

/// An observer already holding the provider's slot keeps the recorder out of
/// both, so no measurement is recorded into a half-installed pipeline.
#[test]
fn a_recorder_is_not_installed_while_another_observer_holds_the_providers_slot() {
    let (foreign, _foreign_seen) = Tally::new();
    assert!(af_providers::observe::install(foreign));
    let (ours, seen) = Tally::new();

    assert!(!record::install(ours));

    record::push_failed(PushFailure::Refused);
    assert!(
        seen.try_recv().is_err(),
        "a recorder refused by the provider's slot records nothing"
    );
}
