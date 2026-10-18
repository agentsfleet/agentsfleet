//! The connect stamp's monotonicity, which the broker's cache fingerprint
//! depends on.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use jiff::Timestamp;

use super::connected_at;

/// The instant `millis` after the epoch: fixtures here are spelled in the
/// milliseconds the stored shape carries.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("the fixture instant is in range")
}

/// Two stamps from one instant are still strictly increasing.
///
/// The property the broker's cache fingerprint depends on: without it, a
/// reconnect to a different account inside one millisecond keeps serving
/// the previous account's cached token — see [`super::LAST_CONNECT_STAMP`].
#[test]
fn two_connects_in_one_millisecond_do_not_share_a_stamp() {
    let instant = at(1_700_000_000_000);

    let first = connected_at(instant);
    let second = connected_at(instant);

    assert!(
        second.as_millisecond() > first.as_millisecond(),
        "{} must follow {}",
        second.as_millisecond(),
        first.as_millisecond(),
    );
}

/// A later wall clock wins over the counter, rather than trailing it.
///
/// The stamp is a real instant that happens to be monotonic, not a
/// sequence: a handle whose `connected_at_ms` drifted away from the clock
/// would misreport when a workspace connected.
#[test]
fn a_later_instant_is_taken_rather_than_the_previous_stamp_plus_one() {
    let early = connected_at(at(1_700_000_000_000));
    let far_later = 1_800_000_000_000;

    let late = connected_at(at(far_later));

    assert!(late.as_millisecond() > early.as_millisecond());
    assert_eq!(late.as_millisecond(), far_later);
}
