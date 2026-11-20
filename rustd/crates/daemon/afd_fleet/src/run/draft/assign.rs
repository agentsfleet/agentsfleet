//! The draft assignment: one transaction under the host lock takes the draft, its
//! thread and its run, and the assignment is rendered from what it committed.
//!
//! Lock order is the host row, then the draft's creator and draft (`NOWAIT`,
//! so a busy draft never stalls the walk), then the draft's thread and its
//! earlier run. A draft's report takes the draft and then the thread, and holds
//! no host row, so the two never wait on each other.

use af_core::id::Uuid7;
use af_wire::{run::RunKind, setup::FailureReason};
use afd_runner::capacity::{Held, choice::Ticket};
use afd_setup::setup_run::{SetupAssignment, SetupHint, SetupOwed};
use jiff::Timestamp;
use sqlx::Connection as _;

use super::DraftAssignment;
use super::issue::Issue;
use crate::error::{Result, query};
use crate::run::{RunOwner, RunService, runner_runs};

/// What query failures name the draft assignment as.
pub(super) const CONTEXT: &str = "assign a draft run";

/// Logged at `info` once a draft run's assignment commits.
const EVENT_ASSIGNED: &str = "setup_run_assigned";

/// What one candidate's assignment came to.
enum Placement {
    Issued(Box<DraftAssignment>),
    /// The host can take no more work this pass.
    Busy,
    /// This draft yields nothing now; the next may.
    Skipped,
}

impl RunService {
    /// Assigns the oldest waiting draft turn `runner` may run, under the
    /// pass's placement `ticket`, and commits its run.
    ///
    /// A draft whose model, balance, stored key or head candidate is at fault
    /// has its turn failed and is passed, so one broken draft never stalls the
    /// walk or the fleet work after it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn assign_draft(
        &self,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        ticket: Ticket,
        now: Timestamp,
    ) -> Result<Option<DraftAssignment>> {
        for hint in drafts.setup_candidates(runner).await? {
            match self.assign_one(drafts, runner, &hint, ticket, now).await? {
                Placement::Issued(assignment) => return Ok(Some(*assignment)),
                Placement::Busy => return Ok(None),
                Placement::Skipped => {}
            }
        }
        Ok(None)
    }

    async fn assign_one(
        &self,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        hint: &SetupHint,
        ticket: Ticket,
        now: Timestamp,
    ) -> Result<Placement> {
        let mut connection = self.runs.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let Some(host) = self
            .runs
            .lock_host(&mut tx, runner, &hint.tenant_id)
            .await?
        else {
            return Ok(Placement::Busy);
        };
        if let Some(refused) = refusal(&host, ticket, runner) {
            return Ok(refused);
        }
        let mut owed = SetupOwed::default();
        let turn = match drafts
            .assign_setup_turn(&mut tx, hint, host.tenant_owned(), &mut owed)
            .await?
        {
            SetupAssignment::Assigned(turn) => *turn,
            SetupAssignment::Passed | SetupAssignment::Busy => {
                commit(tx, owed, drafts, None).await?;
                return Ok(Placement::Skipped);
            }
        };
        // The key the runner calls the model with, opened in this transaction
        // under the same locks that pinned the model, before any run exists.
        let Some(route) = self.route(&mut tx, &turn).await? else {
            drafts
                .fail_setup_turn(&mut tx, &turn, FailureReason::ModelUnavailable, &mut owed)
                .await?;
            commit(tx, owed, drafts, None).await?;
            return Ok(Placement::Skipped);
        };
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let taken = self
            .take_thread(&mut tx, drafts, runner, &turn, &mut owed)
            .await?;
        let Some((won, reassignment)) = taken.won else {
            commit(tx, owed, drafts, taken.expired.as_ref()).await?;
            return Ok(Placement::Skipped);
        };
        let issue = Issue {
            runner,
            owner: &owner,
            turn: &turn,
            fence: won.fencing_token,
            held_until: won.run_expires_at,
            run_version: host.run_version(),
            reassignment,
            max_runtime: afd_setup::limits::SETUP_RUNTIME,
        };
        let Some(run_id) = self.issue_draft(&mut tx, issue, now).await? else {
            // The host filled, the fence or the choice moved: nothing commits.
            return Ok(Placement::Busy);
        };
        drafts.start_setup_turn(&mut tx, &turn).await?;
        commit(tx, owed, drafts, taken.expired.as_ref()).await?;
        runner_runs::acquired(runner);
        log_assigned(&turn.draft_id, &run_id, runner, won.fencing_token);
        let (fence, expires_at) = (won.fencing_token, won.run_expires_at);
        let assignment = DraftAssignment::new(run_id, fence, expires_at, turn, route);
        Ok(Placement::Issued(Box::new(assignment)))
    }
}

/// Why `host` takes no draft from `runner` under `ticket`, if it does not:
/// busy when it is full or the choice moved, skipped when it serves another
/// tenant.
fn refusal(host: &Held, ticket: Ticket, runner: &Uuid7) -> Option<Placement> {
    if !host.can_assign() || !ticket.permits_revision(host.run_version(), RunKind::Setup) {
        return Some(Placement::Busy);
    }
    // Before any write: a foreign runner leaves no run and no failure behind.
    if !host.serves() {
        afd_runner::capacity::refused_placement(
            runner,
            afd_runner::capacity::PlacementRefusal::ForeignTenant,
            RunKind::Setup.as_str(),
        );
        return Some(Placement::Skipped);
    }
    None
}

/// Commits the assignment's transaction, then reports what it owes and counts
/// the lapsed holder whose run it `expired`.
async fn commit(
    tx: sqlx::Transaction<'_, sqlx::Postgres>,
    owed: SetupOwed,
    drafts: &afd_setup::Drafts,
    expired: Option<&Uuid7>,
) -> Result<()> {
    tx.commit().await.map_err(query(CONTEXT))?;
    owed.settle(drafts);
    if let Some(expired) = expired {
        runner_runs::expired(expired);
    }
    Ok(())
}

/// Logs a committed draft run's assignment, once, at `info`.
fn log_assigned(draft: &Uuid7, run: &Uuid7, runner: &Uuid7, fencing_token: i64) {
    let (draft_id, run_id, runner_id) = (draft.as_str(), run.as_str(), runner.as_str());
    tracing::info!(
        event = EVENT_ASSIGNED,
        draft_id,
        run_id,
        runner_id,
        fencing_token
    );
}
