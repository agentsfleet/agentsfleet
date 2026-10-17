//! The wall clock: what it reads, how it cuts to the millisecond, and how a
//! test steers it.
//!
//! The precision is the one that matters here. Postgres stores `TIMESTAMPTZ(3)`
//! and the API writes milliseconds, so a reading carrying a fraction of a
//! millisecond would come back from either as a different instant. And a unit
//! mistake survives every test that only compares two readings from the same
//! function, so the first test pins the reading against real calendar bounds.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, UNIX_EPOCH};

use afd_core::clock::{
    Clock, SystemClock, add_millis, from_millis, instant_at, millis_between, now,
    saturating_millis, saturating_millis_signed, whole_millis,
};
use jiff::Timestamp;

/// 2020-01-01T00:00:00Z. Any reading below this is not the present.
const YEAR_2020_MS: i64 = 1_577_836_800_000;
/// 2100-01-01T00:00:00Z. Any reading above this is not the present either.
const YEAR_2100_MS: i64 = 4_102_444_800_000;

/// Microseconds in one millisecond, for the sub-millisecond cases.
const MICROS_PER_MILLI: i64 = 1_000;

/// Two instants 3.5 seconds apart, for the signed-difference assertions.
const EARLIER_MS: i64 = 1_000;

/// The later of that pair.
const LATER_MS: i64 = 4_500;

/// The instant `millis` after the epoch, for a fixture.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("a fixture instant is in range")
}

/// The instant `micros` after the epoch, for a sub-millisecond fixture.
fn at_micros(micros: i64) -> Timestamp {
    Timestamp::from_microsecond(micros).expect("a fixture instant is in range")
}

/// The reading is the present, in whole milliseconds.
#[test]
fn test_the_reading_is_the_present_in_whole_milliseconds() {
    let reading = now();
    let millis = reading.as_millisecond();
    assert!(millis > YEAR_2020_MS, "{reading} is before 2020");
    assert!(millis < YEAR_2100_MS, "{reading} is after 2100");
    assert_eq!(
        whole_millis(reading),
        reading,
        "{reading} carries a fraction of a millisecond"
    );
}

/// The clock does not run backward between two reads of it.
#[test]
fn test_two_readings_do_not_go_backward() {
    let first = now();
    let second = now();
    assert!(second >= first, "{second} preceded {first}");
}

/// A sub-millisecond instant floors to the millisecond, before 1970 too.
///
/// Floor and truncation agree for every instant after 1970 and disagree for
/// every instant before it, so this is where a silent change would hide.
#[test]
fn test_an_instant_floors_to_the_millisecond() {
    for (micros, expected_ms) in [
        (0_i64, 0_i64),
        (999, 0),
        (MICROS_PER_MILLI, 1),
        (1_999, 1),
        (-1, -1),
        (-999, -1),
        (-MICROS_PER_MILLI, -1),
        (-1_001, -2),
    ] {
        assert_eq!(
            whole_millis(at_micros(micros)).as_millisecond(),
            expected_ms,
            "{micros}µs floored wrong"
        );
    }
}

/// Seconds, as a JWT `exp` reads them, truncate toward zero.
#[test]
fn test_seconds_truncate_toward_zero() {
    assert_eq!(at(1_999).as_second(), 1);
    assert_eq!(at(-1_999).as_second(), -1);
}

/// Arithmetic saturates rather than wrapping.
///
/// A wrapping add turns "far future" into "long past", and the expiry check
/// that reads it then PASSES — the failure mode worth spending a branch on.
#[test]
fn test_arithmetic_saturates_at_the_bounds() {
    assert_eq!(
        add_millis(Timestamp::MAX, 1),
        Timestamp::MAX,
        "an add at the ceiling must not wrap into the past"
    );
    assert_eq!(add_millis(Timestamp::MIN, -1), Timestamp::MIN);
    assert_eq!(add_millis(at(EARLIER_MS), 3_500), at(LATER_MS));
}

/// A millisecond count reads as its instant, saturating past the range.
#[test]
fn test_a_millisecond_count_reads_as_its_instant() {
    assert_eq!(from_millis(LATER_MS), at(LATER_MS));
    assert_eq!(from_millis(-LATER_MS), at(-LATER_MS));
    assert_eq!(from_millis(i64::MAX), Timestamp::MAX);
    assert_eq!(from_millis(i64::MIN), Timestamp::MIN);
}

/// A difference is signed, so "how long ago" and "how far ahead" are one call.
#[test]
fn test_a_difference_carries_its_direction() {
    let earlier = at(EARLIER_MS);
    let later = at(LATER_MS);

    assert_eq!(millis_between(earlier, later), 3_500);
    assert_eq!(millis_between(later, earlier), -3_500);
    assert_eq!(millis_between(later, later), 0);
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
        "{injected} fell outside [{before}, {after}]"
    );
}

/// Clones of the test clock share one reading.
///
/// This is the property the seam exists for: a test hands a clone to the
/// component and keeps one to move time with. Two independent clocks would let
/// the component's view drift from the test's.
#[test]
#[cfg(feature = "test-util")]
fn test_a_fixed_clock_shares_one_reading_across_clones() {
    use afd_core::clock::FixedClock;

    let held = FixedClock::at(at(1_700_000_000_000));
    let handed_over = held.clone();
    assert_eq!(handed_over.now(), at(1_700_000_000_000));

    held.advance_millis(5_000);
    assert_eq!(
        handed_over.now(),
        at(1_700_000_005_000),
        "the clone must see time the test moved"
    );

    held.set(Timestamp::UNIX_EPOCH);
    assert_eq!(handed_over.now(), Timestamp::UNIX_EPOCH);
}

/// The test clock holds whole milliseconds, as the real one does.
#[test]
#[cfg(feature = "test-util")]
fn test_a_fixed_clock_floors_what_it_is_given() {
    use afd_core::clock::FixedClock;

    let clock = FixedClock::at(at_micros(1_999));
    assert_eq!(clock.now(), at(1));
}

/// The test clock can step BACKWARD, because a wall clock does.
///
/// An operator correcting drift, or an NTP step, moves the wall clock back.
/// Code that assumes time only increases breaks exactly there, and it can only
/// be tested if the seam allows the step.
#[test]
#[cfg(feature = "test-util")]
fn test_a_fixed_clock_can_step_backward() {
    use afd_core::clock::FixedClock;

    let clock = FixedClock::at(at(2_000));
    clock.advance_millis(-1_500);
    assert_eq!(clock.now(), at(500));

    clock.set(Timestamp::MIN);
    clock.advance_millis(-1);
    assert_eq!(
        clock.now(),
        whole_millis(Timestamp::MIN),
        "stepping back past the bound saturates rather than wrapping to the future"
    );
}

/// A clock set before 1970 reads as the pre-epoch instant it is.
///
/// This is the reason [`instant_at`] takes the instant instead of reading the
/// clock: no test can set the host clock back to 1969. A silent epoch reading
/// would corrupt `UUIDv7` ordering behind a real instant.
#[test]
fn test_a_pre_epoch_clock_reads_before_the_epoch() {
    assert_eq!(instant_at(UNIX_EPOCH), Timestamp::UNIX_EPOCH);
    assert_eq!(
        instant_at(UNIX_EPOCH + Duration::from_micros(1_500_400)),
        at(1_500)
    );

    let reading = instant_at(UNIX_EPOCH - Duration::from_millis(1_500));
    assert_eq!(
        reading,
        at(-1_500),
        "a pre-epoch host must not be flattened to the epoch"
    );
}

/// A reading past the range a `Timestamp` holds saturates to that end.
#[test]
fn test_an_out_of_range_reading_saturates() {
    let far_future = UNIX_EPOCH + Duration::from_secs(400_000_000_000);
    assert_eq!(instant_at(far_future), whole_millis(Timestamp::MAX));
    if let Some(far_past) = UNIX_EPOCH.checked_sub(Duration::from_secs(400_000_000_000)) {
        assert_eq!(instant_at(far_past), whole_millis(Timestamp::MIN));
    }
}

#[test]
fn test_a_span_reads_in_whole_milliseconds_and_saturates() {
    let span = Duration::from_micros(2_999);

    assert_eq!(saturating_millis(span), 2);
    assert_eq!(saturating_millis_signed(span), 2);
    assert_eq!(saturating_millis(Duration::MAX), u64::MAX);
    assert_eq!(saturating_millis_signed(Duration::MAX), i64::MAX);
}
