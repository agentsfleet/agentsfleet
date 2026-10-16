//! The connect stamp's monotonicity, which the broker's cache fingerprint
//! depends on.

use afd_core::clock::UnixMillis;

use super::connected_at;

/// Two stamps from one instant are still strictly increasing.
///
/// The property the broker's cache fingerprint depends on: without it, a
/// reconnect to a different account inside one millisecond keeps serving
/// the previous account's cached token — see [`super::LAST_CONNECT_STAMP`].
#[test]
fn two_connects_in_one_millisecond_do_not_share_a_stamp() {
    let instant = UnixMillis::from_millis(1_700_000_000_000);

    let first = connected_at(instant);
    let second = connected_at(instant);

    assert!(
        second.as_millis() > first.as_millis(),
        "{} must follow {}",
        second.as_millis(),
        first.as_millis(),
    );
}

/// A later wall clock wins over the counter, rather than trailing it.
///
/// The stamp is a real instant that happens to be monotonic, not a
/// sequence: a handle whose `connected_at_ms` drifted away from the clock
/// would misreport when a workspace connected.
#[test]
fn a_later_instant_is_taken_rather_than_the_previous_stamp_plus_one() {
    let early = connected_at(UnixMillis::from_millis(1_700_000_000_000));
    let far_later = 1_800_000_000_000;

    let late = connected_at(UnixMillis::from_millis(far_later));

    assert!(late.as_millis() > early.as_millis());
    assert_eq!(late.as_millis(), far_later);
}
