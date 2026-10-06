use afd_core::clock::UnixMillis;

use super::next_fire;

/// 2026-03-15T09:04:30Z, in milliseconds.
const MAR_15_0904_30: i64 = 1_773_565_470_000;

/// 2026-03-15T09:05:00Z, the minute after [`MAR_15_0904_30`].
const MAR_15_0905: i64 = MAR_15_0904_30 + 30_000;

/// 2027-03-15T09:05:00Z, the same minute a year on.
const MAR_15_2027_0905: i64 = 1_805_101_500_000;

#[test]
fn a_one_off_minute_still_ahead_matches_this_year() {
    let next = next_fire("5 9 15 3 *", "UTC", UnixMillis::from_millis(MAR_15_0904_30));
    assert_eq!(next, Some(MAR_15_0905));
}

#[test]
fn a_one_off_minute_already_passed_matches_next_year() {
    let next = next_fire("5 9 15 3 *", "UTC", UnixMillis::from_millis(MAR_15_0905));
    assert_eq!(
        next,
        Some(MAR_15_2027_0905),
        "a passed minute matches a year out"
    );
}

#[test]
fn the_expression_is_read_in_the_schedules_zone() {
    // 14:35 in Kolkata (UTC+05:30) is 09:05 UTC.
    let next = next_fire(
        "35 14 15 3 *",
        "Asia/Kolkata",
        UnixMillis::from_millis(MAR_15_0904_30),
    );
    assert_eq!(next, Some(MAR_15_0905));
}

#[test]
fn an_expression_or_zone_that_does_not_read_stores_nothing() {
    let after = UnixMillis::from_millis(MAR_15_0904_30);
    assert_eq!(next_fire("not cron", "UTC", after), None);
    assert_eq!(next_fire("5 9 15 3 *", "Mars/Olympus", after), None);
}
