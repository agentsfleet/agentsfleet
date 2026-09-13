//! The two admission budgets, and the refusal each one is.
//!
//! # Why a budget, when the queue used to trim
//!
//! `MAXLEN ~` bounded a fleet's stream by discarding its oldest entries, so a
//! fleet nobody was draining lost work silently and the producer never knew.
//! Retention now stops at unfinished work, which means a stream that is not
//! draining GROWS — and the ledger row behind every entry grows the table
//! with it. Something has to say no, and it has to say it to the producer,
//! before the row is committed, with a class the producer can back off on.
//!
//! # Two budgets, two questions
//!
//! The FLEET budget is the queue's own backlog: entries delivered and not
//! acknowledged plus entries never delivered, read from the stream itself.
//! One fleet whose consumer died cannot fill the datastore. The DEPLOYMENT
//! budget is the ledger's replay backlog: admitted rows the queue never
//! confirmed. A queue that is refusing appends cannot fill Postgres with rows
//! the sweeper will never drain. The first is a Redis question and the second
//! a Postgres one, which is why they are asked in different places.
//!
//! # A budget that cannot be read is not exceeded
//!
//! A queue that will not answer the backlog question is the outage the ledger
//! exists to survive: the row commits, the caller is answered, and the sweeper
//! appends when the queue is back. Refusing on an unreadable figure would turn
//! the one outage acceptance was designed to ride through into a refusal.

use afd_datastore::FleetStreams;

use crate::Admissions;
use crate::error::{ErrorKind, Result};

/// How many outstanding entries one fleet's stream may hold before its
/// producers are refused.
///
/// The old `MAXLEN ~ 10000` as a refusal instead of a trim: a fleet ten
/// thousand runs behind is not one more message will help.
pub const FLEET_BACKLOG_BUDGET: u64 = 10_000;

/// How many admitted rows may await a receipt across the deployment before
/// every producer is refused.
///
/// The replay sweeper drains at most a batch per pass; a backlog this deep
/// is a queue that has been refusing for a long time, and admitting more
/// only lengthens what it owes.
pub const REPLAY_BACKLOG_BUDGET: u64 = 100_000;

const _: () = {
    assert!(
        FLEET_BACKLOG_BUDGET > 0,
        "a zero budget refuses every fleet"
    );
    assert!(
        REPLAY_BACKLOG_BUDGET > FLEET_BACKLOG_BUDGET,
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
    /// Outstanding entries one fleet's stream may hold.
    pub fleet_backlog: u64,
    /// Unreceipted rows the whole ledger may hold.
    pub replay_backlog: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            fleet_backlog: FLEET_BACKLOG_BUDGET,
            replay_backlog: REPLAY_BACKLOG_BUDGET,
        }
    }
}

/// Which budget refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetScope {
    /// One fleet's stream holds its budget of outstanding entries.
    Fleet,
    /// The ledger holds its budget of rows awaiting a receipt.
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
    /// Refuses when the fleet's stream already holds its budget of
    /// outstanding entries.
    ///
    /// Asked of the stream before the row is committed, so a refusal leaves
    /// nothing behind. A stream that will not answer — or has no group yet —
    /// admits: see the module note.
    pub(crate) async fn refuse_over_fleet_budget(&self, fleet: &str) -> Result<()> {
        let limit = self.budgets.fleet_backlog;
        match FleetStreams::new(self.queue.clone()).backlog(fleet).await {
            Ok(Some(backlog)) if backlog.outstanding().is_some_and(|held| held >= limit) => {
                Err(ErrorKind::OverBudget {
                    scope: BudgetScope::Fleet,
                    limit,
                }
                .into())
            }
            Ok(_within_budget) => Ok(()),
            Err(unread) => {
                let reason = unread.to_string();
                tracing::debug!(fleet_id = fleet, reason, event = "admission_backlog_unread",);
                Ok(())
            }
        }
    }
}
