//! The delivery half: what an assignment that passes its start check becomes.
//!
//! Split from [`super::pull`] at the seam the verb itself has. Everything up
//! to the start check is a sequence of REFUSALS — money, gates, spellings — and
//! everything after it is a construction. The two halves fail differently
//! enough that reading them together obscures both: a refusal writes a
//! terminal row and answers no-work, while a construction failure here means
//! the fleet's own configuration cannot be enforced.

use std::time::Instant;

use af_core::id::Uuid7;
use af_fleet_runtime::config::{Access, RepositoryBinding};
use jiff::Timestamp;

use crate::error::Result;
use crate::run::AssignedRun;
use crate::run::answer::EVENT_ASSIGNED;
use crate::run::envelope::Acquired;
use crate::run::pull::{Assigned, CheckedFleetWork, RunService, Step};
use af_core::turn::label;
use af_wire::run::RunPayload;
use afd_gate::policy::approved::Approved;
use afd_gate::policy::{Network, build, repair};

impl RunService {
    /// Assemble the policy and write the row that makes this runner the holder.
    ///
    /// Named `deliver` rather than `issue`: `Runs::issue` writes the ROW, and
    /// this is the whole delivery around it. Answers [`Step::Go`] only when
    /// that row was written.
    ///
    /// The policy is read from the fleet row and the grants a person approved
    /// for it, fresh for every run: a revoke reaches the next run.
    pub(super) async fn deliver_with<T, F>(
        &self,
        runner_id: &Uuid7,
        checked: CheckedFleetWork,
        now: Timestamp,
        began: Instant,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
        let approved = {
            let mut connection = self.runs.pool().acquire().await?;
            Approved::read_on(&mut connection, &checked.acquired.fleet_id).await?
        };
        let declared = afd_credential::secrets::declared(
            &self.vault,
            &checked.acquired.workspace_id,
            &approved.names(),
            self.broker.connectors(),
        )
        .await?;
        // An unreadable approved scope ends the event in the assembly below.
        let binding = approved.repository_binding().ok().flatten();
        let branch = Self::repair_branch(checked.acquired.turn_id.as_str(), binding.as_ref());
        let policy = match build::assemble(build::Inputs {
            network: Network {
                http_egress: checked.installed.http_egress,
                package_domains: checked.installed.package_domains,
            },
            approved: &approved,
            declared: &declared,
            provider: &checked.resolved,
            repair_branch: branch.as_deref(),
            windows: self.runs.limits().tool_windows,
            judged: checked.acquired.opened.judged,
        }) {
            Ok(policy) => policy,
            // Assembly reads nothing and refuses only an approved scope it
            // cannot enforce, so nothing about the next poll will be
            // different and the event ends. Its detail is stored, never the
            // `[CODE]` rendering.
            Err(misconfigured) => {
                let reason = misconfigured.detail();
                return self
                    .refused(
                        &checked.acquired,
                        label::BINDING_UNENFORCEABLE,
                        runner_id,
                        reason,
                    )
                    .await
                    .map(|()| Step::Stop);
            }
        };
        self.issue_ready(runner_id, &checked, policy, now, began, emit)
            .await
    }

    /// The branch a write-bound run may author on.
    ///
    /// `None` for no binding or a read binding, which needs none.
    ///
    /// # The branch names the EVENT, and the grant is what authorises it
    ///
    /// `uq_integration_grants_fleet_id_service` makes the GitHub grant one row
    /// for the fleet's whole life, so a branch named for the grant would have
    /// every event author on one branch, and a second run would force-update
    /// the first run's head. Authority is the approved grant, read at the
    /// mint, which is the only path to a token. Identity is the event, which
    /// is unique per run — and that is all the branch has to carry, because
    /// `policy::egress` locks whatever name comes back as the one ref the run
    /// may create. No fleet and no workspace in the name: an identifier read
    /// off a public repository says when, never whose.
    fn repair_branch(turn_id: &str, binding: Option<&RepositoryBinding>) -> Option<String> {
        binding
            .filter(|bound| bound.access() == Access::Write)
            .map(|_write| repair::branch_for(turn_id))
    }
}

mod issue;
mod queue_wait;
mod window;

/// The `runner_run_issued` line, with how long the event waited for a runner.
fn log_issued(runner_id: &Uuid7, run_id: &Uuid7, acquired: &Acquired, issued_at: Timestamp) {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner_id = runner_id.as_str();
    let run_id = run_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_turn_id = acquired.turn_id.as_str();
    let queue_wait_ms = queue_wait::of(acquired, issued_at);
    {
        let (event, message) = (&EVENT_ASSIGNED, format_args!("a run was issued"));
        tracing::info!(
            message,
            event,
            runner_id,
            run_id,
            fleet_id,
            agentsfleet_turn_id,
            queue_wait_ms
        );
    };
}

#[cfg(test)]
mod tests;
