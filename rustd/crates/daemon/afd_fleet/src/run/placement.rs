//! One native assignment: a fleet's turn, tagged for the runner.

use af_observability::verb_span::{AssignStage, VerbClock};
use af_wire::run::TaggedRun;

use super::RunService;
use super::{AssignRequest, AssignedRun};
use crate::Result;

impl RunService {
    /// Serialize the assignment inside the values that own its borrowed fields.
    /// The returned metadata carries its target without another database lookup.
    /// Marks each assignment stage it reaches on `clock`; a pass that selects
    /// nothing marks none.
    ///
    /// # Errors
    /// Preserves host selection, policy, billing and durable placement failures.
    pub async fn assign_with<T, F>(
        &self,
        mut request: AssignRequest<'_>,
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
        let Some(ticket) = hosts.assignment_ticket(request.runner).await? else {
            return Ok(None);
        };
        self.assign_fleet_with(&mut request, Some(ticket), clock, |run, metadata| {
            emit(TaggedRun::Fleet(Box::new(run)), metadata)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::run::test_dead;

    // Value: protects=a degraded runner is answered no work before any
    // datastore is asked; fails_when=the degraded guard moves after the host
    // ticket read, which this dead pool refuses; seam=`assign_with` over
    // datastores nobody answers on.
    #[tokio::test]
    async fn a_degraded_runner_is_given_no_run() {
        let mut emitted = 0;

        let answer = test_dead::plane()
            .assign_with(
                crate::run::AssignRequest {
                    runner: &test_dead::id(9),
                    held: &[],
                    degraded: true,
                    credits: 1,
                    now: test_dead::AT,
                    recovery: None,
                },
                &mut af_observability::verb_span::VerbClock::untimed(),
                |_run, _metadata| emitted += 1,
            )
            .await;

        assert!(matches!(answer, Ok(None)), "{:?}", answer.err());
        assert_eq!(emitted, 0, "a degraded runner receives no assignment");
    }
}
