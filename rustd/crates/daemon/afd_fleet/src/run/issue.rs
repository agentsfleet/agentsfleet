//! Writing the run: the durable record that a runner owns a fleet's work.
//!
//! The last write of the run verb, and the one that makes everything before
//! it observable. Until this row exists an assignment is a thread reservation nobody
//! can report against — which is also why the reassignment path is unreachable
//! without it: reassignment looks for a still-`active` run row, and only this
//! writes one.
//!
//! # The metering cursor starts with the row
//!
//! Every run writes its own cursor, `last_metered_at`, in the statement that
//! writes the row, so no run can land over a cursor left from a previous
//! run: the renewal reads it for each slice's delta, and a stale one
//! over-charges the first renewal. A reassigned run is a new row with its own
//! cursor, so it bills only the time its own holder ran.

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::DeliveryStage;
use af_observability::producers;
use af_wire::decision::DecisionAssignment;
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::error::{Result, query};
use crate::run::envelope::Acquired;
use crate::run::runner_runs;
use crate::run::sql;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_ISSUE: &str = "run issue";

/// What the gates resolved, and what the row records about the money.
///
/// Carried in rather than resolved here: the gates resolve what the run bills
/// and then it issues, so the key it billed is the key it delivers — one
/// resolution, no rotation between the two. A reassignment passes the same gates
/// again, so it carries its own resolution rather than the dead holder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Billed<'a> {
    /// The tenant whose wallet answers for this run.
    pub tenant_id: &'a Uuid7,
    /// The billing posture, as its wire spelling.
    pub posture: &'a str,
    /// The provider resolved at billing.
    ///
    /// Stored beside the model so the renew gate and the report settle can key
    /// the rate row by `(provider, model)` without re-resolving.
    pub provider: &'a str,
    /// The model resolved at billing.
    pub model: &'a str,
}

/// The issued run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    /// The run's durable identifier, which the runner reports against.
    pub run_id: Uuid7,
    /// Database timestamp written with the durable run.
    pub created_at: Timestamp,
    /// The exact non-secret selection committed with this run.
    pub decision: Option<Box<DecisionAssignment>>,
    /// Frozen native task and recovery state.
    pub task: Option<af_wire::durable_task::DurableTaskContext<'static>>,
    /// Actual thread deadline after task limits were applied.
    pub expires_at: Timestamp,
}

/// Why the issue wrote no run: the guard that refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The fleet is no longer active in its workspace.
    Scope,
    /// The host row is gone, or this connection no longer proves it.
    HostUnproven,
    /// The host is cordoned, draining, revoked or degraded.
    HostClosed,
    /// The placement choice moved since the assignment read it.
    ChoiceStale,
    /// The host may not take this tenant's work or this model key.
    Placement,
    /// Live runs fill every worker.
    CensusFull(Census),
    /// A worker was free, so the assignment no longer held: its thread
    /// fence, its deadline or the head turn moved under the issue.
    Fenced(Census),
    /// The turn's durable task could not be attached.
    Task,
}

impl Refused {
    /// The log's `reason` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::HostUnproven => "host_unproven",
            Self::HostClosed => "host_closed",
            Self::ChoiceStale => "choice_stale",
            Self::Placement => "placement",
            Self::CensusFull(_) => "census_full",
            Self::Fenced(_) => "fenced",
            Self::Task => "task",
        }
    }

    /// Whether another pass can run the turn: this one lost only for room,
    /// time or the placement choice it read. A refusal of this host, or of the
    /// work itself, would meet the same pass again.
    #[must_use]
    pub const fn rerings(self) -> bool {
        matches!(
            self,
            Self::CensusFull(_) | Self::Fenced(_) | Self::ChoiceStale
        )
    }

    /// The host's census, read when the guarded insert wrote nothing.
    #[must_use]
    pub const fn census(self) -> Option<Census> {
        match self {
            Self::CensusFull(census) | Self::Fenced(census) => Some(census),
            _ => None,
        }
    }
}

/// The host's census beside a refused insert, under the same host lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Census {
    /// Live runs on the host.
    pub occupied: i64,
    /// Other fleets' idle holds on the host, which take no worker.
    pub held: i64,
    /// The host's assigned worker limit.
    pub worker_count: i32,
}

impl Runs {
    /// Write the run row for `acquired`, and the audit row that explains it.
    ///
    /// One statement lands the run, its `run_acquired` event and a fresh run's
    /// meter reset, so an observer can never see a run with no audit row, or a
    /// fresh run metering from a previous run's cursor.
    ///
    /// [`Refused`] when a guard wrote no run, such as an assignment that is no
    /// longer this pass's: its fence was superseded or it lapsed, so another
    /// runner may already hold the same entry. Nothing is written.
    ///
    /// A plain turn reads no task source; a task's turn attaches its task
    /// under the same transaction.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an entropy source that
    /// could not produce an identifier. A full host, moved fence or moved
    /// choice rolls back without issuing a run.
    pub async fn issue(
        &self,
        runner_id: &Uuid7,
        acquired: &Acquired,
        billed: Billed<'_>,
        now: Timestamp,
    ) -> Result<core::result::Result<Issued, Refused>> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ISSUE))?;
        if !crate::durable_task::run::parents(&mut transaction, acquired).await? {
            return Ok(Err(Refused::Scope));
        }
        let Some(host) = self
            .lock_host(&mut transaction, runner_id, billed.tenant_id)
            .await?
        else {
            return Ok(Err(Refused::HostUnproven));
        };
        if let Err(refused) = self.permits_issue(host, runner_id, &billed) {
            return Ok(Err(refused));
        }
        let (run_id, event_row_id) = self.mint(now)?;
        let decision =
            super::decision::freeze(&mut transaction, runner_id, host.decision_selected()).await?;
        let Some(created_at) = (sql::run::RunRow {
            run_id: &run_id,
            runner_id,
            fleet_id: &acquired.fleet_id,
            workspace_id: &acquired.workspace_id,
            tenant_id: billed.tenant_id,
            turn_id: &acquired.turn_id,
            turn_created_at: acquired.turn_created_at,
            posture: billed.posture,
            provider: billed.provider,
            model: billed.model,
            fencing_token: acquired.fence.as_i64(),
            status: sql::RUN_STATUS_ACTIVE,
            event_row_id: &event_row_id,
            kind: acquired.kind.as_str(),
            decision_snapshot: decision.as_deref(),
            run_version: host.run_version(),
            held_until: acquired.held_until,
        })
        .insert(&mut transaction)
        .await?
        else {
            let refused = self
                .refused_insert(&mut transaction, runner_id, acquired, billed.tenant_id)
                .await?;
            return Ok(Err(refused));
        };
        let Some((task, expires_at)) = crate::durable_task::run::attach(
            &mut transaction,
            acquired,
            (runner_id, &run_id),
            self.limits().tasks.slice_seconds(),
        )
        .await?
        else {
            return Ok(Err(Refused::Task));
        };
        transaction.commit().await.map_err(query(CONTEXT_ISSUE))?;
        runner_runs::acquired(runner_id);

        record_issued(&run_id, runner_id, acquired, decision.as_deref());
        Ok(Ok(Issued {
            run_id,
            created_at,
            decision,
            task,
            expires_at,
        }))
    }

    /// Whether the locked host may take this run before the insert counts
    /// its workers and rechecks the assignment's deadline itself.
    fn permits_issue(
        &self,
        host: afd_runner::capacity::Held,
        runner_id: &Uuid7,
        billed: &Billed<'_>,
    ) -> core::result::Result<(), Refused> {
        if !host.can_assign() {
            return Err(Refused::HostClosed);
        }
        if !self
            .ticket()
            .is_none_or(|ticket| ticket.permits_revision(host.run_version()))
        {
            return Err(Refused::ChoiceStale);
        }
        if let Some(refusal) = placement_refusal(host, billed) {
            afd_runner::capacity::refused_placement(runner_id, refusal);
            return Err(Refused::Placement);
        }
        Ok(())
    }

    /// Which guard the insert met, from the host's census in the same
    /// transaction: a full host, or an assignment that no longer held.
    ///
    /// The host row stays locked, so no other issue can fill a worker between
    /// the insert and this read; a run that ended in between can only turn a
    /// full census into [`Refused::Fenced`], never the reverse.
    async fn refused_insert(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner_id: &Uuid7,
        acquired: &Acquired,
        tenant_id: &Uuid7,
    ) -> Result<Refused> {
        let census = self
            .fleet_census(transaction, runner_id, &acquired.fleet_id, tenant_id)
            .await?;
        Ok(census.map_or(Refused::HostUnproven, |capacity| {
            let counted = Census {
                occupied: capacity.occupied(),
                held: capacity.held(),
                worker_count: capacity.workers(),
            };
            if capacity.can_assign() {
                Refused::Fenced(counted)
            } else {
                Refused::CensusFull(counted)
            }
        }))
    }

    /// Draw the independent run and audit identities.
    fn mint(&self, now: Timestamp) -> Result<(Uuid7, Uuid7)> {
        let run_id = self.entropy().uuid7(now)?;
        let event_row_id = self.entropy().uuid7(now)?;
        Ok((run_id, event_row_id))
    }
}

fn record_issued(
    run_id: &Uuid7,
    runner_id: &Uuid7,
    acquired: &Acquired,
    decision: Option<&DecisionAssignment>,
) {
    producers::fleet::delivery_stage(
        DeliveryStage::EventToAssign,
        af_core::clock::elapsed(acquired.turn_created_at, af_core::clock::now()),
    );

    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let run = run_id.as_str();
    let fleet = acquired.fleet_id.as_str();
    let runner = runner_id.as_str();
    let turn_id = acquired.turn_id.as_str();
    let fence = acquired.fence.as_i64();
    let kind = acquired.kind.as_str();
    let decision_model_id = decision.map(|assigned| assigned.profile.model_id.as_str());
    let decision_revision_id = decision.map(|assigned| assigned.revision_id.as_str());
    {
        let (event, run_id, fleet_id, runner_id, agentsfleet_turn_id, fencing_token, message) = (
            &EVENT_RUN_ISSUED,
            &run,
            &fleet,
            &runner,
            &turn_id,
            &fence,
            format_args!("a runner now owns this fleet's work"),
        );
        tracing::debug!(
            message,
            event,
            run_id,
            fleet_id,
            runner_id,
            agentsfleet_turn_id,
            fencing_token,
            kind,
            decision_model_id,
            decision_revision_id
        );
    };
}

/// A run was written and handed to a runner.
///
/// LOG-EVENT (`.orly/standards/domains/logging.md`) value. Dashboards match on this spelling,
/// so changing it is an operator-visible change.
const EVENT_RUN_ISSUED: &str = "run_issued";

/// The placement a tenant's runner may not take: another tenant's work, or work
/// whose model key is the platform's. The assignment filters both; this is the check
/// under the host lock that a hole in that filter cannot pass.
fn placement_refusal(
    host: afd_runner::capacity::Held,
    billed: &Billed<'_>,
) -> Option<afd_runner::capacity::PlacementRefusal> {
    use afd_runner::capacity::PlacementRefusal;
    if !host.serves() {
        return Some(PlacementRefusal::ForeignTenant);
    }
    (host.tenant_owned() && billed.posture == afd_billing::rates::Posture::Platform.as_str())
        .then_some(PlacementRefusal::PlatformModel)
}
