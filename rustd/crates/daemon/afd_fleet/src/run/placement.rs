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

#[cfg(test)]
mod tests {
    use af_wire::run::AcceptedKinds;

    use crate::run::test_dead;

    // Value: protects=a degraded runner is answered no work by the combined
    // setup-and-fleet assignment before any datastore is asked; fails_when=the
    // degraded guard moves after the host ticket read, which this dead pool
    // refuses; why_new=only the fleet arm's own guard was tested; seam=`assign_with`
    // over datastores nobody answers on.
    #[tokio::test]
    async fn a_degraded_runner_is_given_no_run_of_either_kind() {
        let drafts =
            afd_setup::Drafts::new(test_dead::database(), af_crypto::entropy::Entropy::new());
        let mut emitted = 0;

        let answer = test_dead::plane()
            .assign_with(
                &drafts,
                crate::run::AssignRequest {
                    runner: &test_dead::id(9),
                    held: &[],
                    degraded: true,
                    now: test_dead::AT,
                    recovery: None,
                },
                AcceptedKinds::ALL,
                &mut af_observability::verb_span::VerbClock::untimed(),
                |_run, _metadata| emitted += 1,
            )
            .await;

        assert!(matches!(answer, Ok(None)), "{:?}", answer.err());
        assert_eq!(emitted, 0, "a degraded runner receives no assignment");
    }
}
