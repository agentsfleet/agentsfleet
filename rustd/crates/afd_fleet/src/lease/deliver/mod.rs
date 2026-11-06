//! The delivery half: what an admitted claim becomes.
//!
//! Split from [`super::pull`] at the seam the verb itself has. Everything up
//! to admission is a sequence of REFUSALS — money, gates, spellings — and
//! everything after it is a construction. The two halves fail differently
//! enough that reading them together obscures both: a refusal writes a
//! terminal row and answers no-work, while a construction failure here means
//! the fleet's own configuration cannot be enforced.

use std::ops::ControlFlow;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Access;
use afd_wire::policy::ExecutionPolicy;
use futures_util::future;
use jiff::Timestamp;

use crate::error::Result;
use crate::lease::answer::{Context, EVENT_LEASED, render};
use crate::lease::envelope::Acquired;
use crate::lease::history;
use crate::lease::issue::Billed;
use crate::lease::issue::Issue;
use crate::lease::pull::{Ending, Gated, Leased, Plane, claim_lost, issue_denied};
use afd_core::event::label;
use afd_gate::policy::build::{self, Assembled};
use afd_gate::policy::repair;

impl Plane {
    /// Assemble the policy and write the row that makes this runner the holder.
    ///
    /// Named `deliver` rather than `issue`: `Leases::issue` writes the ROW, and
    /// this is the whole delivery around it. Answers `Continue` only when that
    /// row was written.
    pub(super) async fn deliver(
        &self,
        runner_id: &Uuid7,
        admitted: Gated,
        now: Timestamp,
        began: Instant,
    ) -> Result<ControlFlow<Ending, Leased>> {
        // Two independent reads, overlapped: neither nests an acquire, so each
        // keeps its own pooled connection for as long as one statement.
        self.leases.tally().statements(2);
        let (declared, granted) = future::join(
            self.vault.declared(
                &admitted.acquired.workspace_id,
                &admitted.installed.credential_names(),
                self.broker.connectors(),
            ),
            self.gates
                .approved_integrations(&admitted.acquired.fleet_id),
        )
        .await;
        let (declared, granted) = (declared?, granted?);
        let branch = Self::repair_branch(&admitted.acquired.event_id, &admitted.installed.config);

        let policy = match build::assemble(
            build::Inputs {
                config: &admitted.installed.config,
                provider: &admitted.resolved,
                declared: &declared,
                repair_branch: branch.as_deref(),
            },
            &granted,
        ) {
            Ok(Assembled::Ready(policy)) => policy,
            Ok(Assembled::Ungranted {
                credential,
                integration,
            }) => {
                return self
                    .ungranted(runner_id, &admitted.acquired, credential, integration, now)
                    .await;
            }
            // A fleet author's mistake, not an operational fault: nothing about
            // the next poll will be different, so the event ends. Its detail is
            // stored, never the `[CODE]` rendering.
            Err(misconfigured) if misconfigured.is_rejected() => {
                let reason = misconfigured.detail();
                return self
                    .refused(
                        &admitted.acquired,
                        label::BINDING_UNENFORCEABLE,
                        runner_id,
                        reason,
                    )
                    .await
                    .map(|answer| ControlFlow::Break(Ending::Ended(answer)));
            }
            // Anything else is a fault the next poll may not meet, so it is
            // raised rather than ending the event for good.
            Err(fault) => return Err(fault.into()),
        };
        self.issue_ready(runner_id, &admitted, *policy, now, began)
            .await
    }

    async fn issue_ready(
        &self,
        runner_id: &Uuid7,
        admitted: &Gated,
        policy: ExecutionPolicy<'_>,
        now: Timestamp,
        began: Instant,
    ) -> Result<ControlFlow<Ending, Leased>> {
        // LAST, and only once everything above succeeded.
        let issued = match self
            .leases
            .issue(
                runner_id,
                &admitted.acquired,
                Billed {
                    tenant_id: &admitted.billed.tenant_id,
                    posture: admitted.billed.posture.as_str(),
                    provider: &admitted.billed.provider,
                    model: &admitted.billed.model,
                },
                now,
            )
            .await?
        {
            Issue::Issued(issued) => issued,
            Issue::Lost => return claim_lost(runner_id, &admitted.acquired),
            Issue::Denied(reason) => return issue_denied(runner_id, &admitted.acquired, reason),
        };
        // Here, and not at the claim: a claim is an affinity token, and the
        // dozen refusals between it and this line — a stopped fleet, an
        // unparseable event, a denied budget, an unauthorised branch — end
        // without a lease row. Counting one there would make the gauge climb
        // on requests that were refused.
        afd_observability::producers::fleet::runner::lease_taken(runner_id.as_str());
        let issued_at = queue_wait::issued_at(now, began);
        log_issued(runner_id, &issued.lease_id, &admitted.acquired, issued_at);
        // Two independent reads, overlapped: the thread's turns and the
        // fleet's memory window.
        self.leases.tally().statements(2);
        let (history, memory) = future::join(
            history::turns(
                self.thread.as_ref(),
                &admitted.acquired,
                admitted.event_type,
            ),
            self.window_for(&admitted.acquired.fleet_id),
        )
        .await;
        render(
            &issued.lease_id,
            &admitted.acquired,
            admitted.event_type,
            &admitted.installed,
            policy,
            Context { history, memory },
        )
        .map(|answer| ControlFlow::Continue(Leased(answer)))
    }

    /// The branch a write-bound lease may author on.
    ///
    /// `None` for a read binding, which needs none, and `None` for an event
    /// whose identifier is not one this daemon minted — which the assembly then
    /// refuses, because a write binding that cannot name its branch cannot be
    /// turned into rules that bound anything.
    ///
    /// Takes the two values it reads rather than the two structs holding them.
    /// It stopped needing a datastore when the gate lookup went, so the only
    /// thing standing between it and a unit test was a signature asking for
    /// more than it used.
    ///
    /// # The branch names the EVENT, and the grant is what authorises it
    ///
    /// It named the approved repository-write gate until that gate was retired.
    /// The grant could not take its place: `uq_integration_grants_fleet_id_service`
    /// makes it one row for the fleet's whole life, so every event would author
    /// on one branch and a second run would force-update the first run's head.
    ///
    /// So the two questions are answered by two sources. Authority is the
    /// standing grant, read at the mint, which is the only path to a token.
    /// Identity is the event, which is unique per run — and that is all the
    /// branch has to carry, because `policy::egress` locks whatever name comes
    /// back as the one ref the run may create.
    ///
    /// Still no fleet and no workspace in the name: an identifier read off a
    /// public repository says when, never whose.
    fn repair_branch(event_id: &str, config: &FleetConfig) -> Option<String> {
        let binding = config.repository_binding()?;
        if binding.access() != Access::Write {
            return None;
        }
        // The identifier is passed WHOLE. It used to be parsed as a `Uuid7`
        // first, which could not succeed: `afd_admission::admit` is the only
        // thing that mints one and it spells `<millis>-<sequence>`, so every
        // write-bound delivery lost its branch and the assembly refused the
        // event.
        Some(repair::branch_for(event_id))
    }
}

mod queue_wait;
mod ungranted;
mod window;

/// The `runner_lease_issued` line, with how long the event waited for a runner.
fn log_issued(runner_id: &Uuid7, lease_id: &Uuid7, acquired: &Acquired, issued_at: Timestamp) {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner_id = runner_id.as_str();
    let lease_id = lease_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_event_id = acquired.event_id.as_str();
    let queue_wait_ms = queue_wait::of(acquired, issued_at);
    tracing::info!(
        event = EVENT_LEASED,
        runner_id,
        lease_id,
        fleet_id,
        agentsfleet_event_id,
        queue_wait_ms,
        "a lease was issued"
    );
}

#[cfg(test)]
mod tests;
