//! One native assignment selects between independently authorized placement domains.

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
    /// A setup draft's turn rides the common run path
    /// ([`crate::run::draft`]), tagged as a fleet run whose owner is the draft.
    pub async fn assign_with<T, F>(
        &self,
        drafts: &afd_setup::Drafts,
        mut request: AssignRequest<'_>,
        accepted: AcceptedKinds,
        clock: &mut VerbClock<AssignStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(TaggedRun<'a>, AssignedRun) -> T + Send,
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
                RunKind::Setup => {
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
