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
                // Parked, not refused: a human can grant this, and the delivery
                // stays leasable so the next poll picks it up once they have.
                let reason = format!("{credential} needs a grant for {integration}");
                return no_work(runner_id, &reason);
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

/// The credential names a fleet declared, as the vault read wants them.
fn names(installed: &Installed) -> Vec<&str> {
    installed
        .config
        .credentials()
        .iter()
        .map(afd_fleet_runtime::CredentialName::as_str)
        .collect()
}
