//! One native assignment selects between independently authorized placement domains.

use af_core::id::Uuid7;
use af_observability::verb_span::{AssignStage, VerbClock};
use af_wire::run::{AcceptedKinds, RunKind, TaggedRun};

use super::RunService;
use super::{AssignRequest, AssignedRun};
use crate::Result;

impl RunService {
    /// Serialize each domain's assignment inside the values that own its borrowed fields.
    /// The returned metadata carries its target without another database lookup.
    /// Marks each assignment stage it reaches on `clock`; a pass that selects
    /// nothing marks none.
    ///
    /// # Errors
    /// Preserves host selection, policy, billing and durable placement failures.
    ///
    /// `draft_runs` is whether the runner's protocol revision takes a
    /// draft-owned run: such a runner gets setup work on the common run
    /// path ([`crate::run::draft`]), any other the authoring attempt path.
    pub async fn assign_with<T, F>(
        &self,
        drafts: &afd_setup::Drafts,
        mut request: AssignRequest<'_>,
        accepted: AcceptedKinds,
        draft_runs: bool,
        clock: &mut VerbClock<AssignStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(TaggedRun<'a, afd_setup::types::Candidate>, AssignedRun) -> T + Send,
    {
        if request.degraded {
            return Ok(None);
        }
        let hosts = afd_runner::Runners::new(self.runs.pool().clone(), self.runs.entropy().clone());
        let Some(ticket) = hosts.assignment_ticket(request.runner, accepted).await? else {
            return Ok(None);
        };
        for kind in ticket.order() {
            match kind {
                RunKind::Authoring if draft_runs => {
                    clock.restart_if_staged();
                    if let Some(assignment) = self
                        .assign_draft(drafts, request.runner, ticket, request.now)
                        .await?
                    {
                        clock.mark(AssignStage::Select);
                        let metadata = assignment.assigned();
                        let encoded =
                            emit(TaggedRun::Fleet(Box::new(assignment.payload())), metadata);
                        clock.mark(AssignStage::Encode);
                        return Ok(Some(encoded));
                    }
                }
                RunKind::Authoring => {
                    // A fleet assignment the gates refused earlier in this pass
                    // keeps its own span; this assignment starts a fresh one, so
                    // its own database time is its `select` stage.
                    clock.restart_if_staged();
                    if let Some(run) = drafts.assign_next_draft(request.runner, ticket).await? {
                        clock.mark(AssignStage::Select);
                        let metadata = AssignedRun {
                            run_id: Uuid7::parse(&run.run_id)?,
                            expires_at: run.expires_at,
                            target: None,
                            reported: None,
                            draft: None,
                        };
                        let encoded = emit(TaggedRun::Authoring(Box::new(run)), metadata);
                        clock.mark(AssignStage::Encode);
                        return Ok(Some(encoded));
                    }
                }
                RunKind::Fleet => {
                    if let Some(body) = self
                        .assign_fleet_with(&mut request, Some(ticket), clock, |run, metadata| {
                            emit(TaggedRun::Fleet(Box::new(run)), metadata)
                        })
                        .await?
                    {
                        return Ok(Some(body));
                    }
                }
            }
        }
        Ok(None)
    }
}
