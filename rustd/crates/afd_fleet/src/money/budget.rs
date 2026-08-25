//! The fleet's own spend ceiling, and what it has already drained against it.
//!
//! # Spend means money that LEFT the pool
//!
//! Not money metered. On the slice that exhausts a wallet the remainder is
//! forgiven, and a ceiling must count what a tenant was actually charged —
//! otherwise a fleet that ran out of credit would keep accruing against a cap
//! it never spent.
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
//! `budget.zig` is the module that got this right, and this is its rule kept
//! rather than restated: a ceiling we could not READ is not a ceiling we may
//! ignore, but a datastore we could not REACH must not kill every fleet on the
//! platform. So an unreadable budget fails CLOSED and an unavailable one fails
//! OPEN.
//!
//! Here that asymmetry needs no union type, because Rust already has the two
//! channels: a fault is `Err`, and a verdict — including "the stored ceiling
//! will not parse" — is `Ok`. The Zig needs `BudgetRead` with four arms
//! precisely because it has only one channel to say both things through.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Budget;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::money::Nanos;
use crate::money::store::Accounts;
use crate::money::window::Windows;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_DRAIN: &str = "fleet budget drain";

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
/// seeded database. `budget.zig` makes the same point about its own `covers`
/// and then reaches for a `*pg.Conn` two functions later; here the boundary is
/// the function signature.
///
/// The day ceiling is checked first because it is the one that always exists:
/// `monthly_dollars` is optional, and an absent monthly ceiling means no
/// monthly ceiling — not an implicit one derived from the daily.
#[must_use]
pub fn covers(budget: Budget, spend: Spend) -> Verdict {
    if spend.day.has_reached(Nanos::from_dollars(budget.daily())) {
        return Verdict::DayExceeded;
    }
    match budget.monthly() {
        Some(monthly) if spend.month.has_reached(Nanos::from_dollars(monthly)) => {
            Verdict::MonthExceeded
        }
        _ => Verdict::Admit,
    }
}

impl Accounts {
    /// What `fleet_id` has drained inside the windows opening at `now`.
    ///
    /// A fleet with no ledger rows spends zero — the statement's `COALESCE`
    /// answers that, so there is no empty-result branch here.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. The caller applies the
    /// fail-open posture; this does not decide it, which is the whole point of
    /// returning `Result` rather than an `Option` that has already chosen.
    pub async fn spend(
        &self,
        workspace_id: &Uuid7,
        fleet_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Spend> {
        let windows = Windows::at(now);
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::billing::SELECT_BUDGET_DRAIN)
            .bind(workspace_id.as_str())
            .bind(fleet_id.as_str())
            .bind(windows.day.as_millis())
            .bind(windows.month.as_millis())
            .bind(sql::billing::charge::STAGE)
            .bind(sql::billing::charge::RECEIVE)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_DRAIN))?;

        Ok(Spend {
            day: Nanos::from_i64(row.try_get(0).map_err(query(CONTEXT_DRAIN))?),
            month: Nanos::from_i64(row.try_get(1).map_err(query(CONTEXT_DRAIN))?),
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Spend, Verdict, covers};
    use crate::money::{NANOS_PER_USD, Nanos};
    use afd_fleet_runtime::config::Budget;

    /// A ceiling built the only way one can be: through the parser that bounds
    /// it.
    ///
    /// There is no `Budget` constructor, deliberately — a ceiling that did not
    /// come through validation could be negative, infinite, or above the cap,
    /// and every rule in this file assumes it is none of those. So even a unit
    /// test goes the long way round, which is the type doing its job.
    fn ceiling(daily: f64, monthly: Option<f64>) -> Budget {
        let monthly = monthly.map_or(String::new(), |m| format!(r#","monthly_dollars":{m}"#));
        let document = format!(
            r#"{{"name":"b","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],"budget":{{"daily_dollars":{daily}{monthly}}}}}}}"#
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
        assert_eq!(covers(ceiling(5.0, Some(10.0)), spend), Verdict::Admit);
    }

    #[test]
    fn the_day_ceiling_refuses_at_equality_not_past_it() {
        // Exactly five dollars drained against a five-dollar day. The Zig
        // spells this `spend >= cap`, so a fleet that has spent precisely its
        // ceiling runs no further — one more run would be the first dollar of
        // an overdraft nobody authorised.
        let spend = Spend {
            day: Nanos::from_i64(5_000_000_000),
            month: Nanos::ZERO,
        };
        assert_eq!(covers(ceiling(5.0, None), spend), Verdict::DayExceeded);
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
        assert_eq!(covers(ceiling(5.0, None), spend), Verdict::Admit);
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
            covers(ceiling(5.0, Some(10.0)), spend),
            Verdict::MonthExceeded
        );
    }

    #[test]
    fn a_refusal_is_a_refusal_whichever_ceiling_produced_it() {
        assert!(!Verdict::Admit.refuses());
        assert!(Verdict::DayExceeded.refuses());
        assert!(Verdict::MonthExceeded.refuses());
    }
}
