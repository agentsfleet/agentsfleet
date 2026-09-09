//! The delivery half: what an admitted claim becomes.
//!
//! Split from [`super::pull`] at the seam the verb itself has. Everything up
//! to admission is a sequence of REFUSALS — money, gates, spellings — and
//! everything after it is a construction. The two halves fail differently
//! enough that reading them together obscures both: a refusal writes a
//! terminal row and answers no-work, while a construction failure here means
//! the fleet's own configuration cannot be enforced.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Access;
use afd_wire::policy::ExecutionPolicy;

use afd_approval::{Origin, Requested, Wanted};

use crate::error::Result;
use crate::lease::answer::{EVENT_LEASED, no_work, render};
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;
use crate::lease::issue::Billed;
use crate::lease::pull::{Admission2, Plane};
use afd_core::event::label;
use afd_gate::policy::build::{self, Assembled};
use afd_gate::policy::repair;

impl Plane {
    /// Assemble the policy and write the row that makes this runner the holder.
    ///
    /// Named `deliver` rather than `issue`: `Leases::issue` writes the ROW, and
    /// this is the whole delivery around it.
    pub(super) async fn deliver(
        &self,
        runner_id: &Uuid7,
        admitted: Admission2,
        now: UnixMillis,
    ) -> Result<String> {
        let declared = self
            .vault
            .declared(
                &admitted.acquired.workspace_id,
                &names(&admitted.installed),
                &self.connectors,
            )
            .await?;
        let branch = self
            .repair_branch(&admitted.acquired, &admitted.installed)
            .await?;
        let granted = self
            .gates
            .approved_integrations(&admitted.acquired.fleet_id)
            .await?;

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
            // the next poll will be different, so the event ends.
            Err(misconfigured) => {
                let reason = misconfigured.to_string();
                return self
                    .refused(
                        &admitted.acquired,
                        label::BINDING_UNENFORCEABLE,
                        runner_id,
                        &reason,
                        now,
                    )
                    .await;
            }
        };
        self.issue_ready(runner_id, &admitted, *policy, now).await
    }

    /// Ask for the grant this delivery needs, and say what the poll answers.
    ///
    /// The backstop, not the design: a fleet installed after M194 leaves
    /// install with this card already raised, and reaching here means the fleet
    /// predates that or the install-time request could not be written. Asking
    /// again is what makes the invariant unconditional — a park is a question,
    /// never a silent loop — and the request is idempotent, so the one-second
    /// redelivery cadence raises one card rather than sixty a minute.
    ///
    /// A DENIED grant ends the event. The gate that denied it carries no
    /// `event_id` and so could not end anything itself; this is where a
    /// person's no stops the redelivery, and it is the only outcome here that
    /// is not a park.
    async fn ungranted(
        &self,
        runner_id: &Uuid7,
        acquired: &Acquired,
        credential: &str,
        integration: &str,
        now: UnixMillis,
    ) -> Result<String> {
        let asked = self
            .grants
            .request(
                &acquired.workspace_id,
                &acquired.fleet_id,
                Wanted {
                    service: integration,
                    credential,
                    origin: Origin::Park,
                },
                now,
            )
            .await;
        let reason = format!("{credential} needs a grant for {integration}");
        match answers(written(asked, &acquired.fleet_id, integration)) {
            Ungranted::Ends => {
                self.refused(acquired, label::GRANT_DENIED, runner_id, &reason, now)
                    .await
            }
            Ungranted::Waits => no_work(runner_id, &reason),
        }
    }

    async fn issue_ready(
        &self,
        runner_id: &Uuid7,
        admitted: &Admission2,
        policy: ExecutionPolicy<'_>,
        now: UnixMillis,
    ) -> Result<String> {
        // LAST, and only once everything above succeeded.
        let issued = self
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
            .await?;
        // Here, and not at the claim: a claim is an affinity token, and the
        // dozen refusals between it and this line — a stopped fleet, an
        // unparseable event, a denied budget, an unauthorised branch — end
        // without a lease row. Counting one there would make the gauge climb
        // on requests that were refused.
        afd_observability::producers::fleet::runner::lease_taken(runner_id.as_str());
        let runner_id = runner_id.as_str();
        let lease_id = issued.lease_id.as_str();
        let fleet_id = admitted.acquired.fleet_id.as_str();
        let agentsfleet_event_id = admitted.acquired.event_id.as_str();
        tracing::info!(
            event = EVENT_LEASED,
            runner_id,
            lease_id,
            fleet_id,
            agentsfleet_event_id,
            "a lease was issued"
        );
        render(
            &issued.lease_id,
            &admitted.acquired,
            admitted.event_type,
            &admitted.installed,
            policy,
        )
    }

    /// The branch a write-bound lease may author on, if one is authorised.
    ///
    /// `None` for a read binding, which needs none, and `None` for a write
    /// binding with no usable approval — which the assembly then refuses,
    /// because a write binding that cannot name its branch cannot be turned
    /// into rules that bound anything.
    async fn repair_branch(
        &self,
        acquired: &Acquired,
        installed: &Installed,
    ) -> Result<Option<String>> {
        let Some(binding) = installed.config.repository_binding() else {
            return Ok(None);
        };
        if binding.access() != Access::Write {
            return Ok(None);
        }
        Ok(self
            .gates
            .approved_write_gate(&acquired.fleet_id, &acquired.event_id, binding)
            .await?
            .as_ref()
            .map(repair::branch_for))
    }
}

/// A park could not raise its card, and the write failed rather than the human.
const EVENT_REQUEST_FAILED: &str = "park_grant_request_failed";

/// The request's answer, with a failure to write one REPORTED before it is dropped.
///
/// The error dies here either way — [`answers`] treats an unwritten request the
/// same as an open question, because a datastore that would not answer is this
/// instance's problem and reading its silence as a refusal would end deliveries
/// on an outage. What it must not do is die QUIETLY. A park that could not raise
/// its card answers the same `no_work` with the same reason as a park waiting on
/// a person, so without this line the two are indistinguishable in the log while
/// the delivery redelivers every second — which is precisely the failure with no
/// error that this milestone exists to end, reproduced on the path that ends it.
///
/// [`crate::lease::pull::Plane::refused`] already reports the denial and
/// `IntegrationGrants::request` reports what it wrote; the error was the one
/// outcome nothing spoke for. The install path says the same thing at its own
/// call site (`afd_fleet_lifecycle::install::grants`).
fn written(
    asked: Result<Requested, afd_approval::Error>,
    fleet: &Uuid7,
    service: &str,
) -> Option<Requested> {
    match asked {
        Ok(answer) => Some(answer),
        Err(unwritten) => {
            let fleet_id = fleet.as_str();
            let reason = unwritten.to_string();
            tracing::warn!(
                error_code = unwritten.code().as_str(),
                event = EVENT_REQUEST_FAILED,
                fleet_id,
                service,
                reason,
                "the delivery parked without raising its grant card; the next poll asks again"
            );
            None
        }
    }
}

/// What an ungranted delivery does once the grant has been asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ungranted {
    /// The delivery stays leasable and the next poll tries again.
    Waits,
    /// The event ends: nothing about the next poll would be different.
    Ends,
}

/// Whether a park waits for an answer, or ends on one already given.
///
/// The one decision this arm adds, and it has exactly one terminal case. A
/// person's NO is the only outcome that makes the next poll pointless — every
/// other reading leaves a question a human can still answer, and ending an
/// event on any of them would throw away work nobody refused.
///
/// `None` is a request that could not be WRITTEN, and it waits. Fail-closed
/// here means keeping the event alive: a datastore that would not answer is
/// this instance's problem, and reading its silence as a refusal would end
/// deliveries on an outage.
const fn answers(asked: Option<Requested>) -> Ungranted {
    match asked {
        Some(Requested::Denied) => Ungranted::Ends,
        Some(Requested::Raised | Requested::Pending | Requested::Approved) | None => {
            Ungranted::Waits
        }
    }
}

/// The credential names a fleet declared, as the vault read wants them.
fn names(installed: &Installed) -> Vec<&str> {
    installed
        .config
        .credentials()
        .iter()
        .map(afd_fleet_runtime::CredentialName::as_str)
        .collect()
}

#[cfg(test)]
mod tests;
