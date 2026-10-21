//! Both budgets count unsettled admissions. The fleet check holds its fleet row.

use afd_core::error_code::{Coded as _, Logged};
use jiff::Timestamp;

use crate::error::{ErrorKind, Result};
use crate::{Admission, Admissions, sql};

/// Maximum unsettled admissions per fleet, serialized by its row lock.
pub const FLEET_BACKLOG_BUDGET: u64 = 10_000;

/// Deployment backlog ceiling checked against a sampled estimate.
pub const DEPLOYMENT_BACKLOG_BUDGET: u64 = 100_000;

const _: () = {
    assert!(
        FLEET_BACKLOG_BUDGET > 0,
        "a zero budget refuses every fleet"
    );
    assert!(
        DEPLOYMENT_BACKLOG_BUDGET > FLEET_BACKLOG_BUDGET,
        "the deployment must be allowed at least one full fleet"
    );
};

/// The ceilings one ledger admits under.
///
/// Plain numbers at the boundary rather than a `NonZero`, per the workspace's
/// reading of the guideline on type families; the compile-time assertions
/// above hold the defaults, and a suite that wants a small budget says so
/// through [`Admissions::with_budgets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    /// Unsettled admissions one fleet may hold.
    pub fleet_backlog: u64,
    /// Estimated unsettled admissions across the deployment.
    pub deployment_backlog: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            fleet_backlog: FLEET_BACKLOG_BUDGET,
            deployment_backlog: DEPLOYMENT_BACKLOG_BUDGET,
        }
    }
}

/// Which budget refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetScope {
    /// One fleet holds its budget of unsettled admissions.
    Fleet,
    /// The deployment estimate has reached its ceiling.
    Deployment,
}

impl BudgetScope {
    /// The spelling a log line carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Deployment => "deployment",
        }
    }
}

/// Forwards to [`BudgetScope::as_str`], so the error sentence and the log
/// line spell the scope identically.
impl std::fmt::Display for BudgetScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Admissions {
    /// Refresh when due; retain the previous soft estimate if sampling fails.
    pub(crate) async fn deployment_estimate(&self, now: Timestamp) -> u64 {
        let budget = self.budgets.deployment_backlog;
        let estimate = self.ceiling.estimate();
        if !self.ceiling.due(now, estimate, budget) {
            return estimate;
        }
        let claim = self.ceiling.claim();
        if !claim.won() {
            return estimate;
        }
        match self.backlog().await {
            Ok(read) => {
                claim.publish(read.rows, now);
                self.ceiling.estimate()
            }
            Err(unread) => {
                let Logged { error_code, reason } = unread.logged();
                tracing::debug!(error_code, reason, event = "admission_ceiling_unread",);
                estimate
            }
        }
    }

    /// Runs after policy takes the fleet row lock, serializing fresh inserts.
    /// A repeated key is accepted even when either budget is full.
    pub(crate) async fn check_fleet_budget(
        &self,
        connection: &mut sqlx::PgConnection,
        admission: &Admission<'_>,
        key: &str,
    ) -> Result<()> {
        let limit = self.budgets.fleet_backlog;
        let exceeded: bool = sqlx::query_scalar(sql::FLEET_BUDGET)
            .bind(admission.fleet)
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX))
            .bind(admission.producer.as_str())
            .bind(key)
            .fetch_one(connection)
            .await
            .map_err(crate::error::query("fleet admission budget"))?;
        if exceeded {
            return Err(ErrorKind::OverBudget {
                scope: BudgetScope::Fleet,
                limit,
            }
            .into());
        }
        Ok(())
    }
}

mod ceiling;

pub(crate) use self::ceiling::Ceiling;
