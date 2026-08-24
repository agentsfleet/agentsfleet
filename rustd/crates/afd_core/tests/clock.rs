//! The wall clock: what it reads, how it truncates, and how a test steers it.
//!
//! The unit mistake is the one that matters here. Seconds read as milliseconds
//! puts every timestamp in 1970; nanoseconds read as milliseconds puts them
//! past the year 50000 — and both survive every test that only compares a
//! reading against another reading from the same function. So the first test
//! pins the reading against real calendar bounds.
use afd_core::clock::{Clock, SystemClock, UnixMillis, now};

/// 2020-01-01T00:00:00Z. Any reading below this is not milliseconds.
const YEAR_2020_MS: i64 = 1_577_836_800_000;
/// 2100-01-01T00:00:00Z. Any reading above this is not milliseconds either.
const YEAR_2100_MS: i64 = 4_102_444_800_000;

/// The reading is milliseconds since the epoch, in this century.
///
/// This is the test that catches a unit swap, and it is worth more than any
/// comparison of two readings from the same clock: those agree with each other
/// whatever unit they are in.
#[test]
fn test_the_reading_is_epoch_milliseconds_not_seconds_or_nanoseconds() {
    let reading = now().as_millis();
    assert!(
        reading > YEAR_2020_MS,
        "{reading} is before 2020 — a seconds reading read as milliseconds"
    );
    assert!(
        reading < YEAR_2100_MS,
        "{reading} is after 2100 — a nanoseconds or microseconds reading"
    );
}

/// The clock does not run backward between two reads of it.
#[test]
fn test_two_readings_do_not_go_backward() {
    let first = now();
    let second = now();
    assert!(second >= first, "{second:?} preceded {first:?}");
}

/// Seconds truncate toward zero, which is what `clock.zig`'s `nowSeconds` does.
///
/// `@divTrunc`, not `@divFloor`. The two agree for every instant after 1970 and
/// disagree for every instant before it, so a floor here would be a divergence
/// that no test using a present-day timestamp could ever see.
#[test]
fn test_seconds_truncate_toward_zero_like_the_zig_daemon() {
    for (millis, expected) in [
        (0_i64, 0_i64),
        (999, 0),
        (1_000, 1),
        (1_999, 1),
        (-1, 0),
        (-999, 0),
        (-1_000, -1),
        (-1_999, -1),
        (1_577_836_800_123, 1_577_836_800),
    ] {
        assert_eq!(
            UnixMillis::from_millis(millis).as_seconds(),
            expected,
            "{millis}ms truncated wrong"
        );
    }
}

/// Arithmetic saturates rather than wrapping.
///
/// A wrapping add turns "far future" into "long past", and the expiry check
/// that reads it then PASSES — the failure mode worth spending a branch on.
#[test]
fn test_arithmetic_saturates_at_the_bounds() {
    let far_future = UnixMillis::from_millis(i64::MAX);
    assert_eq!(
        far_future.saturating_add_millis(1),
        far_future,
        "an add at the ceiling must not wrap into the past"
    );

    let long_past = UnixMillis::from_millis(i64::MIN);
    assert_eq!(long_past.saturating_add_millis(-1), long_past);
    assert_eq!(
        far_future.saturating_millis_since(long_past),
        i64::MAX,
        "a difference too wide to represent saturates"
    );
}

/// A difference is signed, so "how long ago" and "how far ahead" are one call.
#[test]
fn test_a_difference_carries_its_direction() {
    let earlier = UnixMillis::from_millis(1_000);
    let later = UnixMillis::from_millis(4_500);

    assert_eq!(later.saturating_millis_since(earlier), 3_500);
    assert_eq!(earlier.saturating_millis_since(later), -3_500);
    assert_eq!(later.saturating_millis_since(later), 0);
    assert!(earlier < later, "instants order by the instant");
}

/// The real clock and the free function are the same reading.
#[test]
fn test_the_system_clock_reads_the_same_source_as_the_free_function() {
    let before = now();
    let injected = SystemClock.now();
    let after = now();

    assert!(
        injected >= before && injected <= after,
        "{injected:?} fell outside [{before:?}, {after:?}]"
    );
}

/// Clones of the test clock share one reading.
///
/// This is the property the seam exists for: a test hands a clone to the
/// component and keeps one to move time with. Two independent clocks would let
/// the component's view drift from the test's, and the test would then be
/// asserting against a clock nobody moved.
#[test]
#[cfg(feature = "test-util")]
fn test_a_fixed_clock_shares_one_reading_across_clones() {
    use afd_core::clock::FixedClock;

    let held = FixedClock::at(UnixMillis::from_millis(1_700_000_000_000));
    let handed_over = held.clone();
    assert_eq!(handed_over.now().as_millis(), 1_700_000_000_000);

    held.advance_millis(5_000);
    assert_eq!(
        handed_over.now().as_millis(),
        1_700_000_005_000,
        "the clone must see time the test moved"
    );

    held.set(UnixMillis::EPOCH);
    assert_eq!(handed_over.now(), UnixMillis::EPOCH);
}

/// The test clock can step BACKWARD, because a wall clock does.
///
/// An operator correcting drift, or an NTP step, moves the wall clock back.
/// Code that assumes time only increases — a cache that computes `now - fetched`
/// and reads a negative age as "fresh forever" — breaks exactly there, and it
/// can only be tested if the seam allows the step.
#[test]
#[cfg(feature = "test-util")]
fn test_a_fixed_clock_can_step_backward() {
    use afd_core::clock::FixedClock;

    let clock = FixedClock::at(UnixMillis::from_millis(2_000));
    clock.advance_millis(-1_500);
    assert_eq!(clock.now().as_millis(), 500);

    clock.set(UnixMillis::from_millis(i64::MIN));
    clock.advance_millis(-1);
    assert_eq!(
        clock.now().as_millis(),
        i64::MIN,
        "stepping back past the bound saturates rather than wrapping to the future"
    );
}

/// A clock set before 1970 reads NEGATIVE, the way `clock.zig` does.
///
/// This is the parity claim, and it is the reason [`millis_at`] takes the
/// instant instead of reading the clock: no test can set the host clock back to
/// 1969, so a branch that only the real clock could reach would carry an
/// unchecked claim about how two binaries answer the same broken host.
///
/// The tempting alternative — map the failure to `0` — is what `afd_db` did
/// privately before this module existed, and `clock.zig` rejects it in its own
/// words: *"a silent epoch-0 return would corrupt `UUIDv7` timestamp ordering."*
/// Zero is a real instant, one second into 1970; the reading it replaces is not.
#[test]
fn test_a_pre_epoch_clock_reads_negative_rather_than_zero() {
    use std::time::{Duration, UNIX_EPOCH};

    use afd_core::clock::millis_at;

    assert_eq!(millis_at(UNIX_EPOCH), UnixMillis::EPOCH);
    assert_eq!(
        millis_at(UNIX_EPOCH + Duration::from_millis(1_500)).as_millis(),
        1_500
    );

    let before_epoch = UNIX_EPOCH - Duration::from_millis(1_500);
    let reading = millis_at(before_epoch);
    assert_eq!(
        reading.as_millis(),
        -1_500,
        "a pre-epoch host must not be flattened to the epoch"
    );
    assert_ne!(
        reading,
        UnixMillis::EPOCH,
        "zero is a real instant and must not stand in for a broken clock"
    );
    assert_eq!(
        reading.as_seconds(),
        -1,
        "and the seconds view truncates toward zero, as the Zig daemon does"
    );
}
