//! The reference keys and lifetimes the gate store writes.

use std::time::Duration;

use super::{REF_GRACE, REF_MINIMUM, key, reference_lifetime};

const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";

/// A timeout comfortably beyond the floor, so the grace is what answers.
const FAR_TIMEOUT: Duration = Duration::from_hours(30);

/// A timeout inside the floor, so the clamp is what answers.
const NEAR_TIMEOUT: Duration = Duration::from_secs(1);

/// Whole seconds in `span`, as the key's lifetime counts them.
fn seconds(span: Duration) -> i64 {
    i64::try_from(span.as_secs()).unwrap_or(i64::MAX)
}

#[test]
fn every_key_is_built_from_its_declared_prefix() {
    // Pinned byte for byte: the sweeper, the resolver and the webhook
    // handler read these, and one changed colon makes a pending gate
    // unreachable rather than failing loudly.
    assert_eq!(
        key::event_ref("fleet-1", "event-9"),
        "fleet:gate:byevent:fleet-1:event-9"
    );
    assert_eq!(
        key::response(ACTION),
        format!("fleet:gate:response:{ACTION}")
    );
    assert_eq!(
        key::anomaly("fleet-1", "shell", "run"),
        "fleet:anomaly:fleet-1:shell:run"
    );
}

#[test]
fn a_reference_outlives_its_own_deadline() {
    // The property the grace exists for: the poll that produces the TIMEOUT
    // runs after the deadline, so a key expiring at the deadline would take
    // the reference away before anything could resolve it.
    assert_eq!(
        reference_lifetime(FAR_TIMEOUT),
        seconds(FAR_TIMEOUT + REF_GRACE)
    );
    assert!(reference_lifetime(FAR_TIMEOUT) > seconds(FAR_TIMEOUT));
}

#[test]
fn a_short_or_zero_timeout_is_floored_rather_than_shortened() {
    // A key Dragonfly drops before the resolving poll leaves the event parked
    // with no reference at all.
    assert_eq!(reference_lifetime(NEAR_TIMEOUT), seconds(REF_MINIMUM));
    assert_eq!(reference_lifetime(Duration::ZERO), seconds(REF_MINIMUM));
}

#[test]
fn a_timeout_at_the_edge_of_the_range_saturates_instead_of_overflowing() {
    // An unchecked addition overflows here, and release builds keep overflow
    // checks on, so it would abort the daemon.
    assert_eq!(reference_lifetime(Duration::MAX), i64::MAX);
}
