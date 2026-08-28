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

use crate::error::Result;
use crate::lease::answer::{EVENT_LEASED, no_work, render};
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;
use crate::lease::issue::Billed;
use crate::lease::pull::{Admission2, Plane};
use crate::policy::build::{self, Assembled};
use crate::policy::repair;
use afd_core::event::label;

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
        let Admission2 {
            acquired,
            installed,
            event_type,
            resolved,
            billed,
        } = admitted;

        let declared = self
            .vault
            .declared(&acquired.workspace_id, &names(&installed), &self.connectors)
            .await?;
        let branch = self.repair_branch(&acquired, &installed).await?;
        let granted = self.gates.approved_integrations(&acquired.fleet_id).await?;

        let policy = match build::assemble(
            build::Inputs {
                config: &installed.config,
                provider: &resolved,
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
                        &acquired,
                        label::BINDING_UNENFORCEABLE,
                        runner_id,
                        &reason,
                        now,
                    )
                    .await;
            }
        };

        // LAST, and only once everything above succeeded.
        let issued = self
            .leases
            .issue(
                runner_id,
                &acquired,
                Billed {
                    tenant_id: &billed.tenant_id,
                    posture: billed.posture.as_str(),
                    provider: &billed.provider,
                    model: &billed.model,
                },
                now,
            )
            .await?;
        tracing::info!(
            event = EVENT_LEASED,
            runner_id = runner_id.as_str(),
            lease_id = issued.lease_id.as_str(),
            fleet_id = acquired.fleet_id.as_str(),
            agentsfleet_event_id = acquired.event_id.as_str(),
            "a lease was issued"
        );
        render(&issued.lease_id, &acquired, event_type, &installed, *policy)
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
