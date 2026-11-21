//! A native assignment selects fleet work while retaining the policy borrow.

use af_observability::verb_span::{AssignStage, VerbClock};
use af_wire::run::RunPayload;
use afd_runner::capacity::choice::Ticket;

use super::RunService;
use crate::{
    Result,
    run::{AssignRequest, AssignedRun, answer::no_work},
};

impl RunService {
    /// Place fleet work and invoke the sink while its borrowed policy is alive.
    /// The sink's result remains concrete and cannot borrow from that policy.
    /// Marks each assignment stage it reaches on `clock`, from the selection on.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn assign_fleet_with<T, F>(
        &self,
        request: &mut AssignRequest<'_>,
        ticket: Option<Ticket>,
        clock: &mut VerbClock<AssignStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
        let began = std::time::Instant::now();
        if request.degraded {
            no_work(
                request.runner,
                "the runner's verdict is degraded or unreadable",
            );
            return Ok(None);
        }
        let Some(acquired) = self
            .runs
            .select_ordered(
                request.runner,
                request.held,
                request.recovery.as_deref_mut(),
            )
            .await?
        else {
            no_work(request.runner, "no leasable work");
            return Ok(None);
        };
        clock.mark(AssignStage::Select);
        let mut chosen = self.clone();
        chosen.runs = chosen.runs.with_ticket(ticket);
        chosen
            .run_assigned_with(
                acquired,
                request.runner,
                request.now,
                began,
                clock,
                &mut emit,
            )
            .await
    }
}
