//! The fleet's own spend ceiling, and what it has already drained against it.
//!
//! # Spend means what the fleet's usage cost
//!
//! Read from `billing.usage_lines` through [`crate::limits::fleet_spend`], so a
//! ceiling caps usage whichever biller moves the money. A line the balance
//! could not fully cover still counts at its price: the ceiling bounds what a
//! fleet uses, and the write-off is the credit gate's concern, not this one.
//!
//! # The ceiling is a floor-check, not a projection
//!
//! A run is admitted while `spend < cap`, and refused AT equality. An admitted
//! run may overshoot by at most one renewal window before its next `/renew`
//! refuses it — bounded, not unbounded, and that bound is the design rather
//! than an accident.
//!
//! # Two failures that look alike and point opposite ways
//!
//! A ceiling we could not READ is not a ceiling we may ignore, but a datastore
//! we could not REACH must not kill every fleet on the platform. So an
//! unreadable budget fails CLOSED and an unavailable one fails OPEN.
//!
//! That asymmetry needs no union type, because Rust already has the two
//! channels: a fault is `Err`, and a verdict — including "the stored ceiling
//! will not parse" — is `Ok`.

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Budget;

use crate::Nanos;
use crate::error::Result;
use crate::store::Accounts;

/// What a fleet has drained inside each window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spend {
    /// Drained inside the rolling twenty-four hours.
    pub day: Nanos,
    /// Drained since the UTC month opened.
    pub month: Nanos,
}

/// Whether a fleet may start more work, and which ceiling stopped it.
///
/// The caller maps this onto a log line, never onto a distinct wire code: both
/// ceilings are one `budget_breach` to the operator, and splitting them on the
/// wire would make a dashboard filter on a distinction nobody acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Under both ceilings.
    Admit,
    /// The rolling-day ceiling is reached.
    DayExceeded,
    /// The calendar-month ceiling is reached.
    MonthExceeded,
}

impl Verdict {
    /// Whether this verdict stops the work.
    #[must_use]
    pub const fn refuses(self) -> bool {
        !matches!(self, Self::Admit)
    }

    /// The spelling a log line carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admit => "admit",
            Self::DayExceeded => "day_exceeded",
            Self::MonthExceeded => "month_exceeded",
        }
    }
}

/// Whether `budget` covers `spend`.
///
/// Pure and total — no clock, no connection, no allocation — which is what
/// lets every ceiling case be proven in a unit test rather than against a
/// seeded database. The boundary is the function signature: the connection
/// lives on [`Accounts::spend`], which only supplies the operands.
///
/// The day ceiling is checked first because it is the one that always exists:
/// `monthly_nanos` is optional, and an absent monthly ceiling means no
/// monthly ceiling — not an implicit one derived from the daily.
#[must_use]
pub fn covers(budget: Budget, spend: Spend) -> Verdict {
    if spend.day.has_reached(Nanos::from_ceiling(budget.daily())) {
        return Verdict::DayExceeded;
    }
    match budget.monthly() {
        Some(monthly) if spend.month.has_reached(Nanos::from_ceiling(monthly)) => {
            Verdict::MonthExceeded
        }
        _ => Verdict::Admit,
    }
}

impl Accounts {
    /// What `fleet_id` has spent inside the rolling day and the UTC month.
    ///
    /// Both windows open from the database's `now()`; see
    /// [`crate::limits::fleet_spend`]. A fleet with no usage spends zero.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. The caller applies the
    /// fail-open posture; this does not decide it, which is the whole point of
    /// returning `Result` rather than an `Option` that has already chosen.
    pub async fn spend(&self, workspace_id: &Uuid7, fleet_id: &Uuid7) -> Result<Spend> {
        let mut connection = self.pool().acquire().await?;
        crate::limits::fleet_spend(&mut *connection, workspace_id, fleet_id).await
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Spend, Verdict, covers};
    use crate::{NANOS_PER_USD, Nanos};
    use afd_fleet_runtime::config::Budget;

    /// A ceiling built the only way one can be: through the parser that bounds
    /// it.
    ///
    /// There is no `Budget` constructor, deliberately — a ceiling that did not
    /// come through validation could be negative, infinite, or above the cap,
    /// and every rule in this file assumes it is none of those. So even a unit
    /// test goes the long way round, which is the type doing its job.
    fn ceiling(daily: i64, monthly: Option<i64>) -> Budget {
        let monthly = monthly.map_or(String::new(), |m| format!(r#","monthly_nanos":"{m}""#));
        let document = format!(
            r#"{{"name":"b","x-agentsfleet":{{"triggers":[],"tools":[],"budget":{{"daily_nanos":"{daily}"{monthly}}}}}}}"#
        );
        afd_fleet_runtime::FleetConfig::stored(&document)
            .expect("the fixture document is well-formed")
            .budget()
    }

    #[test]
    fn a_fleet_under_both_ceilings_is_admitted() {
        // One dollar today against a five-dollar cap, two this month against
        // ten — written in the unit the ceiling is authored in rather than as
        // raw nanos, so the comparison being tested is legible without
        // counting zeroes.
        let spend = Spend {
            day: Nanos::from_i64(NANOS_PER_USD),
            month: Nanos::from_i64(2 * NANOS_PER_USD),
        };
        assert_eq!(
            covers(ceiling(5_000_000_000, Some(10_000_000_000)), spend),
            Verdict::Admit
        );
    }

    #[test]
    fn the_day_ceiling_refuses_at_equality_not_past_it() {
        // Exactly five dollars drained against a five-dollar day.
        // `Nanos::has_reached` spells this `spend >= cap`, so a fleet that has
        // spent precisely its ceiling runs no further — one more run would be
        // the first dollar of an overdraft nobody authorised.
        let spend = Spend {
            day: Nanos::from_i64(5_000_000_000),
            month: Nanos::ZERO,
        };
        assert_eq!(
            covers(ceiling(5_000_000_000, None), spend),
            Verdict::DayExceeded
        );
    }

    #[test]
    fn an_absent_monthly_ceiling_is_no_ceiling_rather_than_a_derived_one() {
        // A year of spend against a fleet that declared only a daily cap. The
        // month must not be inferred from the day — an implicit 30× ceiling
        // would refuse runs the author never limited.
        let spend = Spend {
            day: Nanos::ZERO,
            month: Nanos::from_i64(900_000_000_000),
        };
        assert_eq!(covers(ceiling(5_000_000_000, None), spend), Verdict::Admit);
    }

    #[test]
    fn the_month_ceiling_refuses_when_the_day_still_has_room() {
        // The case an order-dependent check would miss: today is quiet, the
        // month is spent.
        let spend = Spend {
            day: Nanos::ZERO,
            month: Nanos::from_i64(10_000_000_000),
        };
        assert_eq!(
            covers(ceiling(5_000_000_000, Some(10_000_000_000)), spend),
            Verdict::MonthExceeded
        );
    }

    #[test]
    fn a_refusal_is_a_refusal_whichever_ceiling_produced_it() {
        assert!(!Verdict::Admit.refuses());
        assert!(Verdict::DayExceeded.refuses());
        assert!(Verdict::MonthExceeded.refuses());
        assert_eq!(Verdict::Admit.as_str(), "admit");
        assert_eq!(Verdict::DayExceeded.as_str(), "day_exceeded");
        assert_eq!(Verdict::MonthExceeded.as_str(), "month_exceeded");
    }
}
