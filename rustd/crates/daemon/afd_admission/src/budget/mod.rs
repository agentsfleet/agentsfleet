//! The deployment ceiling: an estimate of unsettled admissions across every
//! fleet, sampled on a cadence, refusing new work once it reaches the budget.
//! No per-fleet cap exists (`M242_001` §1).

use afd_core::error_code::{Coded as _, Logged};
use jiff::Timestamp;

use crate::Admissions;

/// Deployment backlog ceiling checked against a sampled estimate.
pub const DEPLOYMENT_BACKLOG_BUDGET: u64 = 100_000;

const _: () = assert!(
    DEPLOYMENT_BACKLOG_BUDGET > 0,
    "a zero ceiling refuses every admission"
);

/// The ceiling one ledger admits under.
///
/// Plain numbers at the boundary rather than a `NonZero`, per the workspace's
/// reading of the guideline on type families; the compile-time assertion
/// above holds the default, and a suite that wants a small budget says so
/// through [`Admissions::with_budgets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    /// Estimated unsettled admissions across the deployment.
    pub deployment_backlog: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            deployment_backlog: DEPLOYMENT_BACKLOG_BUDGET,
        }
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
}

mod ceiling;

pub(crate) use self::ceiling::Ceiling;
