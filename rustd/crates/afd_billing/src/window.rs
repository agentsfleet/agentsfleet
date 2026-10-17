//! The two windows a fleet's spend is measured inside.
//!
//! # One instant in, two floors out
//!
//! Both windows derive from ONE caller-supplied instant, and this module never
//! reads a clock: a day floor and a month floor read from two separate clock
//! calls can straddle a tick, so a run could be measured against a day window
//! that opened after the month window it is nested in. Taking the instant as an
//! argument also means every test pins time by value rather than by mocking a
//! clock.
//!
//! # The two windows are different KINDS of question, and only one is a calendar
//!
//! The daily ceiling is documented in `authoring.mdx` as a "Rolling 24-hour
//! dollar ceiling" — not "since midnight". So it is arithmetic: subtract a
//! day's worth of milliseconds. Reaching for a date type here would suggest a
//! calendar boundary that the product deliberately does not have.
//!
//! The monthly ceiling IS a calendar question — it opens at the first instant
//! of the UTC month — and that is the one `jiff` is here for. Answering it by
//! hand means walking epoch-day → year-day → month-day and subtracting the
//! zero-based day index, with a leap-February test to prove the walk. A ceiling
//! that opens on the wrong day is one nobody can reconcile against an invoice,
//! so the walk is imported rather than re-derived.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The rolling daily window's width.
///
/// One day of milliseconds, taken from `afd_core::timing::DAY_MS` rather than
/// declared a second time.
const ROLLING_DAY_MS: i64 = afd_core::timing::DAY_MS;

/// Where each spend window opens.
///
/// Two floors and no widths: every consumer binds these straight into
/// `SELECT_BUDGET_DRAIN` as the two instants its `CASE` arms apportion against,
/// and a width would be a second way to say the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windows {
    /// The rolling 24-hour window's opening instant.
    pub day: Timestamp,
    /// The current UTC month's first instant.
    pub month: Timestamp,
}

impl Windows {
    /// Both floors, for one instant.
    #[must_use]
    pub fn at(now: Timestamp) -> Self {
        Self {
            day: afd_core::clock::add_millis(now, -ROLLING_DAY_MS),
            month: month_floor(now),
        }
    }
}

/// The first instant of the UTC month containing `now`.
///
/// Total, and the two fallbacks are deliberate rather than defensive:
///
/// A non-positive instant answers the epoch: there is no month before the
/// epoch to name, so the floor clamps rather than traps.
///
/// A month start jiff cannot zone answers the epoch too. That widens the month
/// window to everything ever recorded, which makes the gate STRICTER, and it is
/// the one place in this file that leans away from the budget gate's fail-open
/// posture. The lean costs nothing: the first of any month after the epoch is
/// inside jiff's range, so the fallback stands only for the type system.
fn month_floor(now: Timestamp) -> Timestamp {
    if now <= Timestamp::UNIX_EPOCH {
        return Timestamp::UNIX_EPOCH;
    }
    // A civil date, not a mutated instant: `first_of_month` answers with a
    // value of the right KIND, so there is no time-of-day component left over
    // to forget to clear. That is the whole reason this window costs a
    // dependency rather than three lines of field assignment.
    now.to_zoned(TimeZone::UTC)
        .date()
        .first_of_month()
        .to_zoned(TimeZone::UTC)
        .map_or(Timestamp::UNIX_EPOCH, |zoned| zoned.timestamp())
}

#[cfg(test)]
mod tests {
    use super::{ROLLING_DAY_MS, Windows, month_floor};
    use jiff::Timestamp;

    /// 2026-07-10T16:04:00Z, and the first instant of its month.
    ///
    /// Pinned as literals, so the floor is asserted against a fixed value
    /// rather than against the arithmetic under test.
    const MID_JULY_2026: Timestamp = Timestamp::constant(1_783_699_440, 0);
    const JULY_2026_START: Timestamp = Timestamp::constant(1_782_864_000, 0);

    #[test]
    fn the_month_window_opens_at_the_first_instant_of_the_utc_month() {
        assert_eq!(month_floor(MID_JULY_2026), JULY_2026_START);
    }

    #[test]
    fn a_month_start_is_its_own_floor() {
        // Idempotent, which is what makes the drain query's inclusive `>=`
        // filter safe: a charge stamped exactly at the boundary belongs to the
        // month it opens, not the one before.
        assert_eq!(month_floor(JULY_2026_START), JULY_2026_START);
    }

    #[test]
    fn the_last_millisecond_of_a_month_belongs_to_that_month() {
        // 2026-06-30T23:59:59.999Z falls in June, not July — the off-by-one
        // that would silently move a tenant's whole month.
        const JUNE_2026_START: Timestamp = Timestamp::constant(1_780_272_000, 0);
        assert_eq!(
            month_floor(Timestamp::constant(1_782_863_999, 999_000_000)),
            JUNE_2026_START
        );
    }

    #[test]
    fn a_leap_february_does_not_bleed_into_march() {
        // 2024-02-29T23:59:59.999Z → 2024-02-01. The case a hand-written
        // epoch-day walk gets wrong, and the reason this is a crate.
        assert_eq!(
            month_floor(Timestamp::constant(1_709_251_199, 999_000_000)),
            Timestamp::constant(1_706_745_600, 0)
        );
        // And the instant after it is March, proving the leap day did not
        // shift the following month's opening.
        assert_eq!(
            month_floor(Timestamp::constant(1_709_251_200, 0)),
            Timestamp::constant(1_709_251_200, 0)
        );
    }

    #[test]
    fn pre_epoch_instants_clamp_instead_of_trapping() {
        assert_eq!(month_floor(Timestamp::UNIX_EPOCH), Timestamp::UNIX_EPOCH);
        let just_before = afd_core::clock::add_millis(Timestamp::UNIX_EPOCH, -1);
        assert_eq!(month_floor(just_before), Timestamp::UNIX_EPOCH);
    }

    #[test]
    fn the_daily_window_rolls_and_does_not_snap_to_midnight() {
        // The distinction the product documents: the window opens exactly 24h
        // back from the instant asked about, wherever in the day that lands.
        let windows = Windows::at(MID_JULY_2026);
        assert_eq!(windows.day, Timestamp::constant(1_783_613_040, 0));
        // Which is emphatically NOT the day's own midnight — if it were, the
        // ceiling would reset on a boundary rather than sliding.
        assert_ne!(windows.day.as_millisecond() % ROLLING_DAY_MS, 0);
    }

    #[test]
    fn both_floors_come_from_one_instant() {
        // The day floor sits inside the month it is nested in, for any instant
        // past the month's first day — the property that two separate clock
        // reads could break.
        let windows = Windows::at(MID_JULY_2026);
        assert!(windows.day > windows.month);
    }
}
