//! The reference keys and lifetimes the gate store writes.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use super::{MILLIS_PER_SECOND, REF_GRACE_SECONDS, REF_MINIMUM_SECONDS, key, reference_lifetime};
use crate::gate::pending::GateRef;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";

/// The instant every case measures its deadline from.
const NOW_MS: i64 = 0;

/// A deadline comfortably beyond the grace, in seconds.
const FAR_DEADLINE_S: i64 = 100_000;

/// A deadline inside the floor, so the clamp is what answers.
const NEAR_DEADLINE_MS: i64 = 1_000;

/// An instant well past a deadline of zero, so the remaining span is
/// negative and the floor is the only thing keeping the expiry valid.
const LATE_NOW_MS: i64 = 10_000_000;

fn reference(deadline_ms: i64) -> GateRef {
    GateRef::new(
        Uuid7::parse(ACTION).expect("a canonical identifier"),
        UnixMillis::from_millis(deadline_ms),
    )
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
    let now = UnixMillis::from_millis(NOW_MS);
    let far = reference(FAR_DEADLINE_S * MILLIS_PER_SECOND);

    let lifetime = reference_lifetime(&far, now);
    assert!(
        lifetime > FAR_DEADLINE_S,
        "a reference must outlive its deadline, got {lifetime}"
    );
}

#[test]
fn a_deadline_already_past_still_gets_a_usable_lifetime() {
    // A negative expiry is one Dragonfly refuses outright, which would leave
    // the event parked with no reference at all.
    let now = UnixMillis::from_millis(LATE_NOW_MS);
    let stale = reference(NOW_MS);

    assert_eq!(reference_lifetime(&stale, now), REF_MINIMUM_SECONDS);
}

#[test]
fn a_deadline_at_the_edge_of_the_clock_saturates_instead_of_overflowing() {
    // The widest span the two instants can hold, both ways round: an
    // unchecked subtraction overflows on each, and release builds keep
    // overflow checks on, so either would abort the daemon.
    let latest = reference(i64::MAX);
    let earliest_now = UnixMillis::from_millis(i64::MIN);
    assert_eq!(
        reference_lifetime(&latest, earliest_now),
        i64::MAX / MILLIS_PER_SECOND + REF_GRACE_SECONDS
    );

    let earliest = reference(i64::MIN);
    let latest_now = UnixMillis::from_millis(i64::MAX);
    assert_eq!(
        reference_lifetime(&earliest, latest_now),
        REF_MINIMUM_SECONDS
    );
}

#[test]
fn a_near_deadline_is_floored_rather_than_shortened() {
    let now = UnixMillis::from_millis(NOW_MS);
    let soon = reference(NEAR_DEADLINE_MS);

    assert_eq!(reference_lifetime(&soon, now), REF_MINIMUM_SECONDS);
}
