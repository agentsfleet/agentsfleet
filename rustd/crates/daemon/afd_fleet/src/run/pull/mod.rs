//! Fleet placement keeps policy borrows alive through the caller's assignment encoder.
//!
//! Gates run before issue; an empty selection carries no polling response or retry hint.
//! The native session owns delivery and decides when another placement pass is useful.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use jiff::Timestamp;

use crate::error::Result;
use crate::run::answer::no_work;
use crate::run::envelope::{Acquired, HeadKind};
use crate::run::installed::Installed;
use crate::run::start_check::{Billed as Started, Request, StartCheck, money_gates_with};
use crate::run::store::Runs;
use af_core::turn::label;
use afd_billing::Accounts;
use afd_credential::provider::{Providers, Resolved};
use afd_vault::Vault;

#[cfg(feature = "test-util")]
mod assigned;
mod held;
mod refuse;
mod select;
mod step;
#[cfg(all(test, feature = "test-util"))]
mod tests;

pub(in crate::run) use self::step::{Assigned, Step, assignment_lost};

/// A finished event's redelivery was acknowledged and not executed.
const EVENT_TERMINAL_SUPPRESSED: &str = "terminal_redelivery_suppressed";

/// The no-work reason a finished event's redelivery answers.
const REDELIVERED_FINISHED: &str = "the redelivered event had already finished";

/// Everything the run verb acts through.
///
/// A bundle rather than five arguments threaded down every helper. Each field
/// is a handle over a pooled connection, so the whole thing is cheap to hold
/// and cheap to clone — which is what lets the composition root build it once
/// and the request path borrow it.
#[derive(Debug, Clone)]
pub struct RunService {
    /// Assignments, rows, and the narrative log.
    pub runs: Runs,
    /// Native task report evidence, sharing the request-path task owner.
    pub durable_task_reports: Option<Arc<dyn crate::durable_task::DurableTaskReportOwner>>,
    /// Ordinary recovery uses the deployment key even when no file store is configured.
    pub turn_checkpoints: crate::run::turn_checkpoint::TurnCheckpoints,
    /// The payer and the issue-time floor: the reads a money gate makes before
    /// anything moves.
    pub accounts: Accounts,
    /// Checks every run's funds as it renews; logging a line charges it.
    ///
    /// A trait object so the built-in ledger, a queue or a third-party
    /// metering system all sit behind one call site; the daemon chooses which,
    /// and its outage policy, where it builds the plane.
    pub biller: Arc<dyn afd_billing::biller::Biller>,
    /// What a fleet remembers between runs.
    ///
    /// A store of its own rather than a verb on [`Runs`]: the tables are a
    /// different schema written under a different role, and a run store that
    /// could write memory would be a run store that needs that role.
    pub memories: afd_memory::Memories,
    /// Which provider key this run bills against.
    pub providers: Providers,
    /// Where declared credentials are opened.
    pub vault: Vault,
    /// The on-demand credential broker.
    ///
    /// Behind an `Arc` because it holds the process's ONE token cache: the
    /// whole point of the cache is that every request shares it, and a `RunService`
    /// clone that deep-copied it would give each cloned handle its own — which
    /// is a cache that never hits and a single-flight that never single-flights.
    pub broker: Arc<afd_credential::credential::Broker>,
    /// The grants a fleet asks for, and the only writer of them.
    ///
    /// The run path READS approved grants on every delivery and asks for one
    /// here only when a run requests access — a different question and,
    /// deliberately, a different crate. `afd_approval` owns the table because
    /// its resolve moves a row in the same statement that answers the card; a
    /// second writer on this path is a row that statement would have to trust.
    pub grants: afd_approval::IntegrationGrants,
    /// The fleet's thread, which a chat run carries its earlier turns from.
    pub thread: Arc<dyn crate::run::Thread>,
}

/// What the assignment and the gates settled, before the policy is built.
pub(super) struct CheckedFleetWork {
    /// The work, and the fleet it belongs to.
    pub(super) acquired: Acquired,
    /// That fleet as installed.
    pub(super) installed: Installed,
    /// The event's type, proven spellable before anything was written.
    pub(super) event_type: TurnKind,
    /// The provider this run was billed against.
    pub(super) resolved: Resolved,
    /// What the money pass resolved.
    pub(super) billed: Started,
}

impl RunService {
    /// Bind grant and renewal transactions to the physical session that owns this host.
    #[must_use]
    pub fn for_connection(mut self, authority: afd_runner::capacity::authority::Authority) -> Self {
        self.runs = self.runs.for_connection(authority);
        self
    }

    /// Run gates after selection under the thread and turn fences.
    async fn check_assigned(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: Timestamp,
    ) -> Result<Step<CheckedFleetWork>> {
        let installed = match self
            .resolve_installed(&acquired, runner_id)
            .await?
            .proceed()
        {
            Ok(installed) => installed,
            Err(ended) => return Ok(ended),
        };

        // The assignment statement opened the event row and stamped delivery.
        let received = acquired.opened.received;
        let delivery = received.delivery;
        if delivery == crate::run::turn::Delivery::Terminal {
            return self.finished(&acquired, runner_id).await;
        }
        // A run that outlived this many holders will not finish on another,
        // and while it heads the fleet's queue nothing behind it runs. A
        // durable task is exempt: its own run allowance bounds it, and its
        // sweep ends it there with the task itself moved on.
        if acquired.opened.head == HeadKind::Plain
            && acquired.earlier_runs >= self.runs.limits().runs_per_turn()
        {
            return self
                .abandoned(&acquired, runner_id)
                .await
                .map(|()| Step::Stop);
        }
        // The tail's opening bracket, once per row: a redelivery found the row
        // already there, and its watchers already hold the marker. The counters
        // were read after the row landed, because the insert is what moves them.
        if delivery == crate::run::turn::Delivery::First {
            self.runs
                .publish_received(&acquired, received.opened_at, received.counters)
                .await;
        }
        self.billed(acquired, installed, runner_id, now).await
    }

    /// The event's type, its payer, its provider and the money gate, over a
    /// row the pass has opened: the run passes its start check and is billed,
    /// or the pass ends.
    async fn billed(
        &self,
        acquired: Acquired,
        installed: Installed,
        runner_id: &Uuid7,
        now: Timestamp,
    ) -> Result<Step<CheckedFleetWork>> {
        let Some(event_type) = TurnKind::parse(&acquired.event_type) else {
            let reason = acquired.event_type.clone();
            let label = label::EVENT_TYPE_UNSUPPORTED;
            return self
                .refused(&acquired, label, runner_id, &reason)
                .await
                .map(|()| Step::Stop);
        };

        // The payer is the fleet row's tenant, which the assignment read: the
        // provider resolves against it and the gates bill it. The payer gate
        // runs inside `money_gates_with`, over this same found tenant.
        let tenant = acquired.opened.tenant_id.clone();
        let resolved = self
            .providers
            .resolve_observed(
                &tenant,
                &installed.model_ref,
                acquired.opened.platform_key.as_ref(),
            )
            .await?;
        let gated = self.gated(&acquired, tenant, &resolved, now);
        let billed = match gated.await?.started() {
            Ok(billed) => billed,
            Err(declined) => return self.ended(&acquired, declined, runner_id).await,
        };
        Ok(Step::Go(CheckedFleetWork {
            acquired,
            installed,
            event_type,
            resolved,
            billed,
        }))
    }

    /// A durable ending suppresses execution and settles the turn under its fence.
    async fn finished(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
    ) -> Result<Step<CheckedFleetWork>> {
        self.runs.settle_terminal(acquired).await?;
        let fleet_id = acquired.fleet_id.as_str();
        let agentsfleet_turn_id = acquired.turn_id.as_str();
        {
            let (event, message) = (
                &EVENT_TERMINAL_SUPPRESSED,
                format_args!("the redelivered event already has a durable ending"),
            );
            tracing::info!(message, fleet_id, agentsfleet_turn_id, event);
        };
        no_work(runner_id, REDELIVERED_FINISHED);
        Ok(Step::Stop)
    }

    /// The money gate over the tenant the payer gate found.
    async fn gated(
        &self,
        acquired: &Acquired,
        tenant: Uuid7,
        resolved: &Resolved,
        now: Timestamp,
    ) -> Result<StartCheck> {
        let request = Request {
            workspace_id: &acquired.workspace_id,
            fleet_id: &acquired.fleet_id,
            turn_id: &acquired.turn_id,
            posture: resolved.posture,
            provider: resolved.provider.as_ref(),
            model: resolved.model.as_ref(),
        };
        let estimate = self.estimate(acquired, resolved, now).await;
        money_gates_with(
            self.biller.as_ref(),
            Ok(Some(tenant)),
            request,
            estimate.as_ref().copied().map_err(|fault| &**fault),
        )
        .await
    }

    /// The credit gate's floor, priced from the book cached at the catalogue
    /// version the assignment read: a warm replica reads no price.
    async fn estimate(
        &self,
        acquired: &Acquired,
        resolved: &Resolved,
        now: Timestamp,
    ) -> Result<afd_billing::rates::Estimate, Arc<crate::Error>> {
        let pricing = self
            .runs
            .prices()
            .try_get_or_load(
                self.runs.pool(),
                resolved.posture,
                resolved.provider.as_ref(),
                resolved.model.as_ref(),
                Some(acquired.opened.catalogue_version),
            )
            .await?;
        Accounts::estimate_from(pricing.book(), resolved.posture, now)
            .map_err(|fault| Arc::new(crate::Error::from(fault)))
    }
}
