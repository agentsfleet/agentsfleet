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
//! The request body. `LeaseRequest` carries a `wire_version` and this port
//! serves exactly one shape, so there is no negotiation, no downgrade, and no
//! "unsupported version" refusal — that last would need a new registry code,
//! and the registry is single-sourced in Zig. A body naming any version, or no
//! body at all, gets the current shape.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use crate::error::Result;
use crate::lease::admit::{Admission, Billed as Admitted, Refusal, Request, money_gates};
use crate::lease::answer::{EVENT_REFUSED, no_work};
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;
use crate::lease::store::Leases;
use afd_billing::Accounts;
use afd_billing::rates::Posture;
use afd_core::event::label;
use afd_credential::provider::{Providers, Resolved};
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_gate::gate::{Check, Gates};

/// Why a poll came back empty when a gate is waiting on a person.
///
/// Declared once because BOTH arms of the gate pass reach it — the first
/// delivery that raises a gate, and the re-poll that finds it still open — and
/// the two sit ninety lines apart. Two spellings would be two different
/// sentences in the log for one state (RULE UFS).
const AWAITING_APPROVAL: &str = "a human owes an answer";

/// Either the pass continues, or it already has its answer.
///
/// The verb below is a sequence of steps that can each end it, and every
/// ending is the SAME shape — a serialized no-work or refusal. Spelling that
/// as a type lets each step stay a few lines and read as one decision, instead
/// of one function carrying nine early returns and the reader having to hold
/// which of them wrote a row.
pub(super) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop; these are the bytes.
    Stop(String),
}

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
        let admitted = match self.admit(runner_id, now).await? {
            Step::Go(admitted) => admitted,
            Step::Stop(answer) => return Ok(answer),
        };
        self.deliver(runner_id, admitted, now).await
    }

    /// Claim work and run every gate over it.
    ///
    /// Ends the pass on anything that means "not this poll", writing the
    /// terminal row where one is owed.
    async fn admit(&self, runner_id: &Uuid7, now: UnixMillis) -> Result<Step<Admission2>> {
        let Some(acquired) = self.leases.select(runner_id, now).await? else {
            return Ok(Step::Stop(no_work(runner_id, "no leasable work")?));
        };
        // Everything past here holds a CLAIM. Stopping below leaves it to lapse
        // rather than releasing it: the fence has already advanced, so a
        // release would only let a second runner take work this poll decided
        // not to run.
        let Some(installed) = self.leases.installed(&acquired.fleet_id).await? else {
            return Ok(Step::Stop(no_work(
                runner_id,
                "the fleet stopped between selection and claim",
            )?));
        };

        // Write ONE, and it precedes every gate: a refusal needs a row to mark
        // terminal.
        let delivery = self.leases.record_received(&acquired, now).await?;

        // Decided before any gate, because it is decided AFTER the lease row
        // otherwise: a producer from a newer build can write a type this daemon
        // has no execution path for, and that delivery must end rather than be
        // issued to a runner and fail to render.
        let Some(event_type) = EventType::parse(&acquired.event_type) else {
            let reason = acquired.event_type.clone();
            return self
                .refused(
                    &acquired,
                    label::EVENT_TYPE_UNSUPPORTED,
                    runner_id,
                    &reason,
                    now,
                )
                .await
                .map(Step::Stop);
        };

        // The payer, which the provider resolution needs and the money pass
        // resolves again. One extra indexed single-row read on the lease path,
        // accepted so that exactly ONE place decides what an unowned workspace
        // means — the money pass, below, which refuses it.
        let resolved = match self.accounts.payer(&acquired.workspace_id).await? {
            Some(tenant) => Some(self.providers.resolve(&tenant).await?),
            None => None,
        };
        let billed = match self
            .money(&acquired, &installed, resolved.as_ref(), delivery, now)
            .await?
        {
            Admission::Admit(billed) => billed,
            Admission::Refuse(refusal) => {
                return self
                    .refused(&acquired, refusal.label, runner_id, refusal.detail, now)
                    .await
                    .map(Step::Stop);
            }
            Admission::Retry(transient) => {
                return Ok(Step::Stop(no_work(runner_id, transient.at)?));
            }
            Admission::Await(_waiting) => {
                return Ok(Step::Stop(no_work(runner_id, AWAITING_APPROVAL)?));
            }
        };

        if let Some(stop) = self.judged(&acquired, &installed, now).await {
            return self.stopped(&acquired, stop, runner_id, now).await;
        }
        // Admitted implies a payer, and a payer implies a resolution — the
        // money pass refuses the workspace that has neither.
        let Some(resolved) = resolved else {
            return Ok(Step::Stop(no_work(runner_id, "admitted with no provider")?));
        };
        Ok(Step::Go(Admission2 {
            acquired,
            installed,
            event_type,
            resolved,
            billed,
        }))
    }

    /// The money pass, over one claim.
    async fn money(
        &self,
        acquired: &Acquired,
        installed: &Installed,
        resolved: Option<&Resolved>,
        delivery: crate::lease::event::Delivery,
        now: UnixMillis,
    ) -> Result<Admission> {
        let (provider, model) =
            resolved.map_or(("", ""), |it| (it.provider.as_ref(), it.model.as_ref()));
        money_gates(
            &self.accounts,
            Request {
                workspace_id: &acquired.workspace_id,
                fleet_id: &acquired.fleet_id,
                event_id: &acquired.event_id,
                event_created_at: acquired.event_created_at,
                budget: installed.config.budget(),
                posture: resolved.map_or(Posture::Platform, |it| it.posture),
                provider,
                model,
                delivery,
            },
            now,
        )
        .await
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

    /// A gate answer that ends the pass.
    async fn stopped(
        &self,
        acquired: &Acquired,
        stop: Admission,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Step<Admission2>> {
        let answer = match stop {
            Admission::Refuse(refusal) => {
                self.refused(acquired, refusal.label, runner_id, refusal.detail, now)
                    .await?
            }
            Admission::Retry(transient) => no_work(runner_id, transient.at)?,
            Admission::Await(_waiting) => no_work(runner_id, AWAITING_APPROVAL)?,
            // `of_gate` answers `None` for a pass, so this arm is the enum
            // being exhaustive rather than a state that occurs.
            Admission::Admit(_) => no_work(runner_id, "a passing gate cannot also stop")?,
        };
        Ok(Step::Stop(answer))
    }

    /// End the event, then answer no-work.
    ///
    /// The refusal is written before the answer, and whether a row MOVED is not
    /// checked: an already-terminal row is a redelivery whose earlier
    /// acknowledgement was lost, and the runner is told the same thing either
    /// way.
    pub(super) async fn refused(
        &self,
        acquired: &Acquired,
        label: &'static str,
        runner_id: &Uuid7,
        reason: &str,
        now: UnixMillis,
    ) -> Result<String> {
        self.leases
            .block(
                &acquired.fleet_id,
                &acquired.event_id,
                Refusal { label, detail: "" },
                now,
            )
            .await?;
        tracing::warn!(
            event = EVENT_REFUSED,
            runner_id = runner_id.as_str(),
            fleet_id = acquired.fleet_id.as_str(),
            agentsfleet_event_id = acquired.event_id.as_str(),
            label,
            reason,
            "the event was ended at a gate"
        );
        no_work(runner_id, label)
    }
}
