use jiff::Timestamp;

use super::next_fire;

/// 2026-03-15T09:04:30Z.
const MAR_15_0904_30: Timestamp = Timestamp::constant(1_773_565_470, 0);

/// 2026-03-15T09:05:00Z, the minute after [`MAR_15_0904_30`].
const MAR_15_0905: Timestamp = Timestamp::constant(1_773_565_500, 0);

/// 2027-03-15T09:05:00Z, the same minute a year on.
const MAR_15_2027_0905: Timestamp = Timestamp::constant(1_805_101_500, 0);

/// 2026-03-15T09:04:59.999Z, one millisecond short of [`MAR_15_0905`].
const MAR_15_0904_59_999: Timestamp = Timestamp::constant(1_773_565_499, 999_000_000);

#[test]
fn a_one_off_minute_still_ahead_matches_this_year() {
    let next = next_fire("5 9 15 3 *", "UTC", MAR_15_0904_30);
    assert_eq!(next, Some(MAR_15_0905));
}

#[test]
fn a_one_off_minute_already_passed_matches_next_year() {
    let next = next_fire("5 9 15 3 *", "UTC", MAR_15_0905);
    assert_eq!(
        next,
        Some(MAR_15_2027_0905),
        "a passed minute matches a year out"
    );
}

#[test]
fn a_stamp_inside_the_minute_before_still_matches_this_year() {
    // A row stamped a millisecond before the minute: the stamp the instant is
    // counted from carries milliseconds, and they must not push it a year.
    let next = next_fire("5 9 15 3 *", "UTC", MAR_15_0904_59_999);
    assert_eq!(next, Some(MAR_15_0905));
}

#[test]
fn the_expression_is_read_in_the_schedules_zone() {
    // 14:35 in Kolkata (UTC+05:30) is 09:05 UTC.
    let next = next_fire("35 14 15 3 *", "Asia/Kolkata", MAR_15_0904_30);
    assert_eq!(next, Some(MAR_15_0905));
}

#[test]
fn an_expression_or_zone_that_does_not_read_stores_nothing() {
    assert_eq!(next_fire("not cron", "UTC", MAR_15_0904_30), None);
    assert_eq!(
        next_fire("5 9 15 3 *", "Mars/Olympus", MAR_15_0904_30),
        None
    );
}
