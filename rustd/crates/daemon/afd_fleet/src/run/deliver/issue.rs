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
        let Some(issued) = self
            .runs
            .issue_for_revision(
                runner,
                &checked.acquired,
                Billed {
                    tenant_id: &checked.billed.tenant_id,
                    posture: checked.billed.posture.as_str(),
                    provider: &checked.billed.provider,
                    model: &checked.billed.model,
                },
                now,
                checked.installed.issue_revision.as_ref(),
            )
            .await?
        else {
            return Ok(assignment_lost(runner, &checked.acquired));
        };
        if checked.acquired.kind == crate::run::Kind::Reassigned {
            self.runs
                .publish_restarted(&checked.acquired, &issued)
                .await;
        }
        self.runs
            .latest()
            .observe(
                &checked.acquired.fleet_id,
                checked
                    .installed
                    .issue_revision
                    .as_ref()
                    .map(|proof| &proof.current),
                checked.acquired.opened.catalogue_version,
            )
            .await;
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
        policy: ExecutionPolicy<'_>,
        issued: Issued,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
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
            history::turns(self.thread.as_ref(), &checked.acquired, checked.event_type),
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
        draft: None,
    }
}
