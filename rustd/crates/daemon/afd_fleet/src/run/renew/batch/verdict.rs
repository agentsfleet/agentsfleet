//! What one run's row of the batch statement decided, and how a refusal is
//! said.

use jiff::Timestamp;

use super::rows::Found;
use crate::error::{Result, budget_exhausted, run_cancelled, run_lost, run_max_runtime};
use crate::run::coverage::EVENT_BUDGET_BREACH;

/// Logged when the batch refuses a run the per-run path would also refuse
/// without a record: the run is lost, cancelled or at its runtime ceiling.
const EVENT_REFUSED: &str = "renew_batch_refused";

/// A run's answer, or the per-run path's to give.
pub(super) enum Answer {
    Done(Result<Timestamp>),
    PerRun,
}

/// One run's row of the batch statement.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Judged {
    pub(super) run_id: af_core::id::Uuid7,
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

/// What one run's row decided.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Extended(Timestamp),
    Lost,
    Cancelled,
    MaxRuntime,
    OverBudget,
    /// Its prices or ceilings moved under the statement: it takes the
    /// per-run path, and the replica reads the fleet's versions again.
    Moved,
    /// The run runs a durable task, which the statement never renews: it
    /// takes the per-run path, which checks the task's authority. A plain
    /// run gains a task when the run binds one, after the session kept its
    /// facts as a plain run's.
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
    /// The order of these tests is the per-run path's: a run that is not
    /// this runner's live holder is lost before anything else is asked of it.
    pub(super) fn verdict(&self) -> Verdict {
        let held = |check: Option<bool>| check.unwrap_or(false);
        if self.found && held(self.has_task) {
            return Verdict::Tasked;
        }
        if !self.found || !held(self.host_renews) {
            return Verdict::Lost;
        }
        // The cancel mark before the turn's admission, as the per-run path asks
        // (`extend.rs`): Stop settles the turn it marks, and its run must hear
        // `cancelled`, not `lost`.
        if held(self.cancelled) {
            return Verdict::Cancelled;
        }
        if !held(self.admitted) {
            return Verdict::Lost;
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
            // Both rows moved, or the run is lost: a deadline the slot does
            // not honour would run the child past a reclaim.
            Some(until) if self.slot_moved => Verdict::Extended(until.to_jiff()),
            _ => Verdict::Lost,
        }
    }
}

/// The refusal a run's verdict answers with.
pub(super) fn refusal_of(verdict: &Verdict) -> crate::Error {
    match verdict {
        Verdict::Cancelled => run_cancelled(),
        Verdict::MaxRuntime => run_max_runtime(),
        Verdict::OverBudget => budget_exhausted(),
        Verdict::Lost | Verdict::Extended(_) | Verdict::Moved | Verdict::Tasked => run_lost(),
    }
}

/// Says why the batch refused a run.
///
/// A fleet at its ceiling is the per-run path's `renew_budget_breach`, under
/// the same name. The other refusals the per-run path answers without a
/// record, and an operator asking why a run stopped needs one, so the batch
/// names them under one event with the verdict as a field.
pub(super) fn log_refused(row: &Found, verdict: &Verdict, refusal: &crate::Error) {
    let fleet = row.fleet_id.as_str();
    let run = row.id.as_str();
    let code = refusal.code().as_str();
    match verdict {
        Verdict::OverBudget => {
            let (fleet_id, run_id, error_code, event, message) = (
                &fleet,
                &run,
                &code,
                &EVENT_BUDGET_BREACH,
                format_args!("the fleet reached a ceiling its author declared; not renewed"),
            );
            tracing::warn!(message, fleet_id, run_id, error_code, event);
        }
        Verdict::Lost | Verdict::Cancelled | Verdict::MaxRuntime => {
            let which = verdict.label();
            {
                let (fleet_id, run_id, error_code, verdict, event, message) = (
                    &fleet,
                    &run,
                    &code,
                    &which,
                    &EVENT_REFUSED,
                    format_args!("the renewal was refused"),
                );
                tracing::info!(message, fleet_id, run_id, error_code, verdict, event);
            };
        }
        Verdict::Extended(_) | Verdict::Moved | Verdict::Tasked => {}
    }
}
