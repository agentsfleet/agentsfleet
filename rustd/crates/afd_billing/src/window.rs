//! The two windows a fleet's spend is measured inside.
//!
//! # One instant in, two floors out
//!
//! Both windows derive from ONE caller-supplied instant, and this module never
//! reads a clock. `budget.zig` makes the same rule and says why: a day floor
//! and a month floor read from two separate `nowMillis()` calls can straddle a
//! tick, so a run could be measured against a day window that opened after the
//! month window it is nested in. Taking the instant as an argument also means
//! every test pins time by value rather than by mocking a clock.
//!
//! # The two windows are different KINDS of question, and only one is a calendar
//!
//! The daily ceiling is documented in `authoring.mdx` as a "Rolling 24-hour
//! dollar ceiling" — not "since midnight". So it is arithmetic: subtract a
//! day's worth of milliseconds. Reaching for a date type here would suggest a
//! calendar boundary that the product deliberately does not have.
//!
//! The monthly ceiling IS a calendar question — it opens at the first instant
//! of the UTC month — and that is the one `jiff` is here for. `clock.zig`
//! answers it by hand, walking epoch-day → year-day → month-day and subtracting
//! the zero-based day index, and it carries a leap-February test to prove the
//! walk. A ceiling that opens on the wrong day is one nobody can reconcile
//! against an invoice, so the walk is imported rather than re-derived.

use afd_core::clock::UnixMillis;
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The rolling daily window's width.
///
/// `budget.zig`'s `ROLLING_DAY_MS`, which is `std.time.ms_per_day`.
const ROLLING_DAY_MS: i64 = 24 * 60 * 60 * 1_000;

/// Where each spend window opens.
///
/// Two floors and no widths: every consumer binds these straight into
/// `SELECT_BUDGET_DRAIN` as the two instants its `CASE` arms apportion against,
/// and a width would be a second way to say the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windows {
    /// The rolling 24-hour window's opening instant.
    pub day: UnixMillis,
    /// The current UTC month's first instant.
    pub month: UnixMillis,
}

impl Windows {
    /// Both floors, for one instant.
    #[must_use]
    pub fn at(now: UnixMillis) -> Self {
        Self {
            day: now.saturating_add_millis(-ROLLING_DAY_MS),
            month: month_floor(now),
        }
    }
}

/// The first instant of the UTC month containing `now`.
///
/// Total, and the two fallbacks are deliberate rather than defensive:
///
/// A non-positive instant answers the epoch, which is `startOfUtcMonthMillis`'s
/// own `if (now_ms <= 0) return 0` — there is no month before the epoch to
/// name, and the Zig chose clamping over trapping.
///
/// An instant no calendar can place answers the epoch too. That widens the
/// month window to everything ever recorded, which makes the gate STRICTER, and
/// it is the one place in this file that leans away from the budget gate's
/// fail-open posture. The lean is affordable because the branch is unreachable
/// with a working clock: `jiff` spans years -9999 to 9999, so reaching it means
/// the host's clock reported an instant tens of millennia from now, and a
/// deployment in that state has a larger problem than one fleet's ceiling.
fn month_floor(now: UnixMillis) -> UnixMillis {
    if now.as_millis() <= 0 {
        return UnixMillis::EPOCH;
    }
    Timestamp::from_millisecond(now.as_millis())
        .ok()
        // A civil date, not a mutated instant: `first_of_month` answers with a
        // value of the right KIND, so there is no time-of-day component left
        // over to forget to clear. That is the whole reason this window costs a
        // dependency rather than three lines of field assignment.
        .map(|instant| instant.to_zoned(TimeZone::UTC).date().first_of_month())
        .and_then(|first| first.to_zoned(TimeZone::UTC).ok())
        .map_or(UnixMillis::EPOCH, |zoned| {
            UnixMillis::from_millis(zoned.timestamp().as_millisecond())
        })
}

#[cfg(test)]
mod tests {
    use super::{ROLLING_DAY_MS, Windows, month_floor};
    use afd_core::clock::UnixMillis;

    /// 2026-07-10T16:04:00Z, and the first instant of its month.
    ///
    /// The same pair `clock.zig`'s own test pins, carried across so the two
    /// implementations are asserted against one literal rather than against
    /// each other's arithmetic.
    const MID_JULY_2026: i64 = 1_783_699_440_000;
    const JULY_2026_START: i64 = 1_782_864_000_000;

    #[test]
    fn the_month_window_opens_at_the_first_instant_of_the_utc_month() {
        assert_eq!(
            month_floor(UnixMillis::from_millis(MID_JULY_2026)),
            UnixMillis::from_millis(JULY_2026_START)
        );
    }

    #[test]
    fn a_month_start_is_its_own_floor() {
        // Idempotent, which is what makes the drain query's inclusive `>=`
        // filter safe: a charge stamped exactly at the boundary belongs to the
        // month it opens, not the one before.
        let start = UnixMillis::from_millis(JULY_2026_START);
        assert_eq!(month_floor(start), start);
    }

    #[test]
    fn the_last_millisecond_of_a_month_belongs_to_that_month() {
        // 2026-06-30T23:59:59.999Z falls in June, not July — the off-by-one
        // that would silently move a tenant's whole month.
        const JUNE_2026_START: i64 = 1_780_272_000_000;
        assert_eq!(
            month_floor(UnixMillis::from_millis(JULY_2026_START - 1)),
            UnixMillis::from_millis(JUNE_2026_START)
        );
    }

    #[test]
    fn a_leap_february_does_not_bleed_into_march() {
        // 2024-02-29T23:59:59.999Z → 2024-02-01. The case a hand-written
        // epoch-day walk gets wrong, and the reason this is a crate.
        assert_eq!(
            month_floor(UnixMillis::from_millis(1_709_251_199_999)),
            UnixMillis::from_millis(1_706_745_600_000)
        );
        // And the instant after it is March, proving the leap day did not
        // shift the following month's opening.
        assert_eq!(
            month_floor(UnixMillis::from_millis(1_709_251_200_000)),
            UnixMillis::from_millis(1_709_251_200_000)
        );
    }

    #[test]
    fn pre_epoch_instants_clamp_instead_of_trapping() {
        assert_eq!(month_floor(UnixMillis::EPOCH), UnixMillis::EPOCH);
        assert_eq!(month_floor(UnixMillis::from_millis(-1)), UnixMillis::EPOCH);
    }

    #[test]
    fn the_daily_window_rolls_and_does_not_snap_to_midnight() {
        // The distinction the product documents: the window opens exactly 24h
        // back from the instant asked about, wherever in the day that lands.
        let now = UnixMillis::from_millis(MID_JULY_2026);
        let windows = Windows::at(now);
        assert_eq!(
            windows.day,
            UnixMillis::from_millis(MID_JULY_2026 - ROLLING_DAY_MS)
        );
        // Which is emphatically NOT the day's own midnight — if it were, the
        // ceiling would reset on a boundary rather than sliding.
        assert_ne!(windows.day.as_millis() % ROLLING_DAY_MS, 0);
    }

    #[test]
    fn both_floors_come_from_one_instant() {
        // The day floor sits inside the month it is nested in, for any instant
        // past the month's first day — the property that two separate clock
        // reads could break.
        let windows = Windows::at(UnixMillis::from_millis(MID_JULY_2026));
        assert!(windows.day.as_millis() > windows.month.as_millis());
    }
}
