//! The lease verb: one runner's poll, from claim to answer.
//!
//! Every step below already exists and is proven on its own. What is here is
//! the ORDER, which is the part no single step can be right about — the money
//! gates must not run before the narrative log exists to record a refusal on,
//! the policy must not assemble before the gates have passed, and the row must
//! not be written before the policy is known to be buildable.
//!
//! # Why this answers serialized bytes
//!
//! `ExecutionPolicy` borrows from the config, the resolved provider and the
//! declared credentials — every field is a `Cow`, which is what keeps the
//! payload copy-free on the path every lease takes. A value borrowing from
//! four locals cannot be returned, and both alternatives are worse than they
//! look: deep-owning the tree copies exactly what the borrows exist to avoid,
//! and assembling twice — once to check, once to render — puts a second
//! opinion about what a run may do into the one place that must have only one.
//!
//! So the assembly, the payload and the serialization all happen inside the
//! borrow, and the caller receives bytes. The HTTP layer adds a status and a
//! content type; it makes no decision, which is what the split is for.
//!
//! # Every stop answers the same thing
//!
//! No work, refused, parked, degraded, nothing ready — all of them are
//! `{"lease":null,"retry_after_ms":…}` and a `200`. The runner's only move is
//! to wait and ask again, so the reasons live in the log where an operator can
//! read them beside a request id, and the wire carries none of them.
//!
//! # What is deliberately not read
//!
//! The request body, which is empty. This port serves exactly one shape, so
//! there is no negotiation, no downgrade, and no "unsupported version" refusal
//! — that last would need a new registry code. Any body, or none, gets that
//! shape.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use crate::error::Result;
use crate::lease::admit::{Admission, Billed as Admitted, Request, money_gates, payer_gate};
use crate::lease::answer::no_work;
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;
use crate::lease::store::Leases;
use afd_billing::Accounts;
use afd_core::event::label;
use afd_credential::provider::{Providers, Resolved};
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_gate::gate::{Check, Gates};

#[cfg(feature = "test-util")]
mod claimed;
mod held;
mod refuse;
mod step;
#[cfg(all(test, feature = "test-util"))]
mod tests;

pub(in crate::lease) use self::step::{Leased, Step, claim_lost};

/// A finished event's redelivery could not be acknowledged.
const EVENT_TERMINAL_ACK_FAILED: &str = "terminal_redelivery_ack_failed";

/// A finished event's redelivery was acknowledged and not executed.
const EVENT_TERMINAL_SUPPRESSED: &str = "terminal_redelivery_suppressed";

/// The no-work reason a finished event's redelivery answers.
const REDELIVERED_FINISHED: &str = "the redelivered event had already finished";

/// Everything the lease verb acts through.
///
/// A bundle rather than five arguments threaded down every helper. Each field
/// is a handle over a pooled connection, so the whole thing is cheap to hold
/// and cheap to clone — which is what lets the composition root build it once
/// and the request path borrow it.
#[derive(Debug, Clone)]
pub struct Plane {
    /// Claims, rows, and the narrative log.
    pub leases: Leases,
    /// Approval gates and standing integration grants.
    pub gates: Gates,
    /// Wallets, ceilings and the receive debit.
    pub accounts: Accounts,
    /// What a fleet remembers between runs.
    ///
    /// A store of its own rather than a verb on [`Leases`]: the tables are a
    /// different schema written under a different role, and a lease store that
    /// could write memory would be a lease store that needs that role.
    pub memories: crate::memory::Memories,
    /// Which provider key this run bills against.
    pub providers: Providers,
    /// Where declared credentials are opened.
    pub vault: Vault,
    /// The on-demand credential broker.
    ///
    /// Behind an `Arc` because it holds the process's ONE token cache: the
    /// whole point of the cache is that every request shares it, and a `Plane`
    /// clone that deep-copied it would give each cloned handle its own — which
    /// is a cache that never hits and a single-flight that never single-flights.
    pub broker: std::sync::Arc<afd_credential::credential::Broker>,
    /// The standing grants a fleet holds, and the only writer of them.
    ///
    /// The lease path READS grants through [`Plane::gates`] on every delivery
    /// and writes one here only when it finds none — which is a different
    /// question and, deliberately, a different crate. `afd_approval` owns the
    /// table because its resolve moves a row in the same statement that answers
    /// a gate; a second writer on this path is a row that statement would have
    /// to trust.
    pub grants: afd_approval::IntegrationGrants,
    /// The connector set a mintable credential is classified against.
    ///
    /// A field rather than an argument: which connectors this daemon ships
    /// with is a composition-root fact, and threading it down from the HTTP
    /// layer would make an accident of it — a handler is the last place that
    /// should get a vote on which third parties exist. The seam for a
    /// different set is [`Vault::declared`], which still takes the trait.
    pub connectors: Registry,
}

/// What the claim and the gates settled, before the policy is built.
pub(super) struct Admission2 {
    /// The work, and the fleet it belongs to.
    pub(super) acquired: Acquired,
    /// That fleet as installed.
    pub(super) installed: Installed,
    /// The event's type, proven spellable before anything was written.
    pub(super) event_type: EventType,
    /// The provider this run was billed against.
    pub(super) resolved: Resolved,
    /// What the money pass resolved.
    pub(super) billed: Admitted,
}

impl Plane {
    /// Answer one runner's poll.
    ///
    /// The bytes are a complete `LeaseResponse` — work, or `null` with a
    /// backoff hint. Never a 204, and never an error for "nothing to do": a
    /// runner polling an idle deployment is the common case, not a fault.
    ///
    /// `degraded` fails CLOSED. A runner whose verdict could not be read is
    /// treated as degraded and issued nothing, because its assignment names an
    /// isolation the host may not deliver and a lease would run outside the
    /// boundary an operator assigned.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored configuration
    /// this daemon cannot read. Every DECISION is an `Ok`.
    pub async fn lease(
        &self,
        runner_id: &Uuid7,
        degraded: bool,
        now: UnixMillis,
    ) -> Result<String> {
        if degraded {
            return no_work(runner_id, "the runner's verdict is degraded or unreadable");
        }
        let Some(acquired) = self.leases.select(runner_id, now).await? else {
            return no_work(runner_id, "no leasable work");
        };
        self.run_claimed(acquired, runner_id, now).await
    }

    /// Every gate over one already-claimed event.
    ///
    /// Ends the pass on anything that means "not this poll", writing the
    /// terminal row where one is owed. Split from the selection because the
    /// two fail for different reasons and are proven differently: WHICH event
    /// a poll gets is the readiness index's decision, and what then happens to
    /// it is this chain's. The suite enters below the selection through
    /// [`Self::lease_claimed`], naming its own fleet, instead of polling a
    /// process-global partition cursor until that fleet comes up.
    async fn admit_claimed(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Step<Admission2>> {
        let installed = match self
            .resolve_installed(&acquired, runner_id, now)
            .await?
            .proceed()
        {
            Ok(installed) => installed,
            Err(ended) => return Ok(ended),
        };

        let received = self.leases.record_received(&acquired, now).await?;
        let delivery = received.delivery;
        if delivery == crate::lease::event::Delivery::Terminal {
            return self.finished(&acquired, runner_id).await;
        }
        // The tail's opening bracket, once per row: a redelivery found the row
        // already there, and its watchers already hold the marker. The counters
        // were read after the row landed, because the insert is what moves them.
        if delivery == crate::lease::event::Delivery::First {
            self.leases
                .publish_received(&acquired, received.opened_at, received.counters)
                .await;
        }
        self.billed(acquired, installed, delivery, runner_id, now)
            .await
    }

    /// The event's type, its payer, its provider and every gate, over a row
    /// the pass has opened: the run is admitted and billed, or the pass ends.
    async fn billed(
        &self,
        acquired: Acquired,
        installed: Installed,
        delivery: crate::lease::event::Delivery,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Step<Admission2>> {
        let Some(event_type) = EventType::parse(&acquired.event_type) else {
            let reason = acquired.event_type.clone();
            let label = label::EVENT_TYPE_UNSUPPORTED;
            return self
                .refused(&acquired, label, runner_id, &reason, now)
                .await
                .map(Step::Stop);
        };

        // The payer is read ONCE, here: the provider resolves against it and
        // the gates bill it, so a second read could only disagree with the first.
        let read = self.accounts.payer(&acquired.workspace_id).await;
        let tenant = match payer_gate(read, &acquired.workspace_id)? {
            Ok(tenant) => tenant,
            Err(declined) => return self.ended(&acquired, declined, runner_id, now).await,
        };
        let resolved = self.providers.resolve(&tenant).await?;
        let gated = self.gated(&acquired, &installed, tenant, &resolved, delivery, now);
        let billed = match gated.await?.admitted() {
            Ok(billed) => billed,
            Err(declined) => return self.ended(&acquired, declined, runner_id, now).await,
        };
        Ok(Step::Go(Admission2 {
            acquired,
            installed,
            event_type,
            resolved,
            billed,
        }))
    }

    /// Dimension 7.4: the event already ran. Acknowledge the entry and stop
    /// BEFORE any of the gates, the money, the secrets or the lease row,
    /// because every one of them is an effect of executing, and the execution
    /// already happened: the tenant paid for it, and its answer is already
    /// owed or delivered.
    ///
    /// Acknowledging is the whole point rather than a tidy-up. An entry left
    /// pending is offered again, so a terminal event that is only SKIPPED
    /// comes back on the next poll forever. A failed acknowledgement is logged,
    /// not propagated: the entry stays pending and this runs again, which is
    /// the same answer one turn later, and failing the lease would refuse a
    /// runner that has done nothing wrong.
    async fn finished(&self, acquired: &Acquired, runner_id: &Uuid7) -> Result<Step<Admission2>> {
        let fleet_id = acquired.fleet_id.as_str();
        let agentsfleet_event_id = acquired.event_id.as_str();
        if let Err(failure) = self
            .leases
            .acknowledge(&acquired.fleet_id, &acquired.receipt)
            .await
        {
            let code = failure.code().as_str();
            let reason = failure.to_string();
            tracing::warn!(
                error_code = code,
                fleet_id,
                agentsfleet_event_id,
                reason,
                event = EVENT_TERMINAL_ACK_FAILED,
                "a finished event was not acknowledged; it will be offered again"
            );
        }
        tracing::info!(
            fleet_id,
            agentsfleet_event_id,
            event = EVENT_TERMINAL_SUPPRESSED,
            "a redelivered event had already finished; it was acknowledged, not executed"
        );
        no_work(runner_id, REDELIVERED_FINISHED).map(Step::Stop)
    }

    /// The money gates over the tenant the payer gate found, then — once they
    /// admit — the approval gate.
    async fn gated(
        &self,
        acquired: &Acquired,
        installed: &Installed,
        tenant: Uuid7,
        resolved: &Resolved,
        delivery: crate::lease::event::Delivery,
        now: UnixMillis,
    ) -> Result<Admission> {
        let request = Request {
            workspace_id: &acquired.workspace_id,
            fleet_id: &acquired.fleet_id,
            event_id: &acquired.event_id,
            event_created_at: acquired.event_created_at,
            budget: installed.config.budget(),
            posture: resolved.posture,
            provider: resolved.provider.as_ref(),
            model: resolved.model.as_ref(),
            delivery,
        };
        let money = money_gates(&self.accounts, Ok(Some(tenant)), request, now).await?;
        if !matches!(money, Admission::Admit(_)) {
            return Ok(money);
        }
        Ok(self.judged(acquired, installed, now).await.unwrap_or(money))
    }

    /// The approval gate, as an admission answer.
    async fn judged(
        &self,
        acquired: &Acquired,
        installed: &Installed,
        now: UnixMillis,
    ) -> Option<Admission> {
        let verdict = self
            .gates
            .check(
                Check {
                    fleet_id: &acquired.fleet_id,
                    workspace_id: &acquired.workspace_id,
                    event_id: &acquired.event_id,
                    event_type: &acquired.event_type,
                    actor: &acquired.actor,
                    request_json: &acquired.request_json,
                    config: &installed.config,
                },
                now,
            )
            .await;
        Admission::of_gate(verdict)
    }
}
