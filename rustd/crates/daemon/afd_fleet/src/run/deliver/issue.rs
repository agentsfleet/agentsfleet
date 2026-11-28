//! Freeze the durable assignment target and serialize while the policy still borrows its inputs.

use std::time::Instant;

use af_core::id::Uuid7;
use af_wire::{policy::ExecutionPolicy, run::RunPayload};
use futures_util::future;
use jiff::Timestamp;

use crate::{
    Result,
    run::{
        AssignedRun, Billed, Issued, Target,
        answer::{Context, payload},
        history,
        pull::{Assigned, CheckedFleetWork, RunService, Step, assignment_lost},
    },
};

impl RunService {
    pub(super) async fn issue_ready<T, F>(
        &self,
        runner: &Uuid7,
        checked: &CheckedFleetWork,
        policy: ExecutionPolicy<'_>,
        now: Timestamp,
        began: Instant,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
        let issued = match self
            .runs
            .issue(
                runner,
                &checked.acquired,
                Billed {
                    tenant_id: &checked.billed.tenant_id,
                    posture: checked.billed.posture.as_str(),
                    provider: &checked.billed.provider,
                    model: &checked.billed.model,
                },
                now,
            )
            .await?
        {
            Ok(issued) => issued,
            Err(refused) => return Ok(assignment_lost(runner, &checked.acquired, refused)),
        };
        if checked.acquired.kind == crate::run::Kind::Reassigned {
            self.runs
                .publish_restarted(&checked.acquired, &issued)
                .await;
        }
        self.runs
            .latest()
            .observe(checked.acquired.opened.catalogue_version);
        af_observability::producers::fleet::runner::run_taken(runner.as_str());
        super::log_issued(
            runner,
            &issued.run_id,
            &checked.acquired,
            super::queue_wait::issued_at(now, began),
        );
        self.render_assignment(runner, checked, policy, issued, emit)
            .await
    }

    async fn render_assignment<T, F>(
        &self,
        runner: &Uuid7,
        checked: &CheckedFleetWork,
        mut policy: ExecutionPolicy<'_>,
        issued: Issued,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
        withhold_task_tools(&mut policy.tools, issued.decision.is_some());
        let unfinished_tasks =
            if issued.decision.is_some() && issued.task.is_none() && checked.acquired.opened.judged
            {
                let mut connection = self.runs.pool().acquire().await?;
                crate::durable_task::inputs::unfinished(
                    &mut connection,
                    &checked.acquired.fleet_id,
                    &checked.acquired.workspace_id,
                )
                .await?
            } else {
                Vec::new()
            };
        let turn_resume = if issued.task.is_none() {
            self.runs
                .turn_resume(super::super::turn_checkpoint::TurnRun {
                    runner_id: runner,
                    run_id: &issued.run_id,
                    fencing_token: checked.acquired.fence.as_u64(),
                })
                .await?
        } else {
            None
        };
        let finished_calls = self
            .runs
            .finished_calls(
                &checked.acquired.fleet_id,
                &checked.acquired.turn_id,
                &issued.run_id,
            )
            .await?;
        let (history, memory) = future::join(
            self.assignment_history(checked, turn_resume.is_some()),
            self.window_for(&checked.acquired),
        )
        .await;
        let metadata = metadata(&issued, checked);
        let body = history::fit_frame(payload(
            &issued.run_id,
            &checked.acquired,
            checked.event_type,
            &checked.installed,
            policy,
            Context {
                history,
                memory,
                decision: issued.decision,
                task: issued.task,
                turn_resume,
                finished_calls,
                expires_at: issued.expires_at,
                unfinished_tasks,
            },
        ));
        Ok(Step::Go(Assigned(emit(body, metadata))))
    }
}

fn metadata(issued: &Issued, checked: &CheckedFleetWork) -> AssignedRun {
    let acquired = &checked.acquired;
    AssignedRun {
        run_id: issued.run_id.clone(),
        expires_at: issued.expires_at,
        target: Some(Target {
            fleet_id: acquired.fleet_id.clone(),
            turn_id: acquired.turn_id.clone(),
            run_created_at: issued.created_at,
            turn_created_at: acquired.turn_created_at,
            timing_eligible: true,
            fence: acquired.fence.as_i64(),
        }),
        // The columns `select_run_for_report.sql` reads, as this issue
        // wrote them. A durable task's run keeps none: it renews on the
        // per-run path, which reads its task's authority.
        reported: issued.task.is_none().then(|| crate::run::Reported {
            fleet_id: acquired.fleet_id.clone(),
            workspace_id: acquired.workspace_id.clone(),
            tenant_id: checked.billed.tenant_id.clone(),
            turn_id: acquired.turn_id.clone(),
            actor: acquired.actor.clone(),
            posture: checked.billed.posture.as_str().to_owned(),
            provider: checked.billed.provider.to_string(),
            model: checked.billed.model.to_string(),
            fence: acquired.fence,
            run_created_at: issued.created_at,
            turn_created_at: acquired.turn_created_at,
            catalogue_version: Some(acquired.opened.catalogue_version),
        }),
    }
}

/// The task tools need a judge: a run that carries no decision profile — its
/// host is not task-capable, or the profile went between the assignment and
/// the issue — is offered none.
fn withhold_task_tools(tools: &mut Vec<std::borrow::Cow<'_, str>>, judged: bool) {
    if !judged {
        tools.retain(|tool| !afd_gate::policy::catalog::TASK_TOOLS.contains(&tool.as_ref()));
    }
}

#[cfg(test)]
mod tests {
    use afd_gate::policy::catalog::{TASK_TOOLS, offered};

    use super::withhold_task_tools;

    fn task_tools(tools: &[std::borrow::Cow<'_, str>]) -> usize {
        tools
            .iter()
            .filter(|tool| TASK_TOOLS.contains(&tool.as_ref()))
            .count()
    }

    // Value: protects=a run with no judge is offered no task tool, whether its
    // host is not task-capable or the run carries no decision profile;
    // fails_when=the catalog offers task tools unjudged, or the issue keeps
    // them for a run with no profile; seam=the catalog and the issue's filter.
    #[test]
    fn a_run_without_a_decision_profile_is_offered_no_task_tools() {
        let mut incapable = offered(false);
        let mut unprofiled = offered(true);
        let mut judged = offered(true);

        withhold_task_tools(&mut incapable, false);
        withhold_task_tools(&mut unprofiled, false);
        withhold_task_tools(&mut judged, true);

        assert_eq!(task_tools(&incapable), 0);
        assert_eq!(task_tools(&unprofiled), 0);
        assert_eq!(task_tools(&judged), TASK_TOOLS.len());
        assert_eq!(unprofiled.len(), incapable.len(), "only the task tools go");
    }
}
