//! What one lease's row of the batch statement decided, and how a refusal is
//! said.

use jiff::Timestamp;

use super::rows::Found;
use crate::error::{Result, budget_exhausted, lease_cancelled, lease_lost, lease_max_runtime};
use crate::lease::coverage::EVENT_BUDGET_BREACH;

/// Logged when the batch refuses a lease the per-lease path would also refuse
/// without a record: the lease is lost, cancelled or at its runtime ceiling.
const EVENT_REFUSED: &str = "renew_batch_refused";

/// A lease's answer, or the per-lease path's to give.
pub(super) enum Answer {
    Done(Result<Timestamp>),
    PerLease,
}

/// One lease's row of the batch statement.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Judged {
    pub(super) lease_id: af_core::id::Uuid7,
    pub(super) found: bool,
    pub(super) cancelled: Option<bool>,
    pub(super) fence_holds: Option<bool>,
    pub(super) below_cap: Option<bool>,
    pub(super) priced_as_held: Option<bool>,
    pub(super) ceilings_current: Option<bool>,
    pub(super) over_budget: Option<bool>,
    pub(super) host_renews: Option<bool>,
    pub(super) admitted: Option<bool>,
    pub(super) has_task: Option<bool>,
    pub(super) extended_until: Option<jiff_sqlx::Timestamp>,
    pub(super) slot_moved: bool,
}

/// What one lease's row decided.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Extended(Timestamp),
    Lost,
    Cancelled,
    MaxRuntime,
    OverBudget,
    /// Its prices or ceilings moved under the statement: it takes the
    /// per-lease path, and the replica reads the fleet's versions again.
    Moved,
    /// The lease runs a durable task, which the statement never renews: it
    /// takes the per-lease path, which checks the task's authority. A plain
    /// lease gains a task when the run binds one, after the session kept its
    /// facts as a plain lease's.
    Tasked,
}

impl Verdict {
    /// How a refusal reads in a log record.
    const fn label(&self) -> &'static str {
        match self {
            Self::Lost => "lost",
            Self::Cancelled => "cancelled",
            Self::MaxRuntime => "max_runtime",
            Self::OverBudget => "over_budget",
            Self::Extended(_) => "extended",
            Self::Moved => "moved",
            Self::Tasked => "tasked",
        }
    }
}

impl Judged {
    /// The order of these tests is the per-lease path's: a lease that is not
    /// this runner's live holder is lost before anything else is asked of it.
    pub(super) fn verdict(&self) -> Verdict {
        let held = |check: Option<bool>| check.unwrap_or(false);
        if self.found && held(self.has_task) {
            return Verdict::Tasked;
        }
        if !self.found || !held(self.host_renews) || !held(self.admitted) {
            return Verdict::Lost;
        }
        if held(self.cancelled) {
            return Verdict::Cancelled;
        }
        if !held(self.fence_holds) {
            return Verdict::Lost;
        }
        if !held(self.below_cap) {
            return Verdict::MaxRuntime;
        }
        if !held(self.priced_as_held) || !held(self.ceilings_current) {
            return Verdict::Moved;
        }
        if held(self.over_budget) {
            return Verdict::OverBudget;
        }
        match self.extended_until {
            // Both rows moved, or the lease is lost: a deadline the slot does
            // not honour would run the child past a reclaim.
            Some(until) if self.slot_moved => Verdict::Extended(until.to_jiff()),
            _ => Verdict::Lost,
        }
    }
}

/// The refusal a lease's verdict answers with.
pub(super) fn refusal_of(verdict: &Verdict) -> crate::Error {
    match verdict {
        Verdict::Cancelled => lease_cancelled(),
        Verdict::MaxRuntime => lease_max_runtime(),
        Verdict::OverBudget => budget_exhausted(),
        Verdict::Lost | Verdict::Extended(_) | Verdict::Moved | Verdict::Tasked => lease_lost(),
    }
}

/// Says why the batch refused a lease.
///
/// A fleet at its ceiling is the per-lease path's `renew_budget_breach`, under
/// the same name. The other refusals the per-lease path answers without a
/// record, and an operator asking why a run stopped needs one, so the batch
/// names them under one event with the verdict as a field.
pub(super) fn log_refused(row: &Found, verdict: &Verdict, refusal: &crate::Error) {
    let fleet = row.fleet_id.as_str();
    let lease = row.id.as_str();
    let code = refusal.code().as_str();
    match verdict {
        Verdict::OverBudget => tracing::warn!(
            fleet_id = fleet,
            lease_id = lease,
            error_code = code,
            event = EVENT_BUDGET_BREACH,
            "the fleet reached a ceiling its author declared; not renewed"
        ),
        Verdict::Lost | Verdict::Cancelled | Verdict::MaxRuntime => {
            let which = verdict.label();
            tracing::info!(
                fleet_id = fleet,
                lease_id = lease,
                error_code = code,
                verdict = which,
                event = EVENT_REFUSED,
                "the renewal was refused"
            );
        }
        Verdict::Extended(_) | Verdict::Moved | Verdict::Tasked => {}
    }
}
