//! Freeze the durable offer target and serialize while the policy still borrows its inputs.

use std::time::Instant;

use af_core::id::Uuid7;
use af_wire::{policy::ExecutionPolicy, run::RunPayload};
use futures_util::future;
use jiff::Timestamp;

use crate::{
    Result,
    run::{
        Billed, Issued, Offered, Target,
        answer::{Context, payload},
        history,
        pull::{AdmittedFleetWork, Assigned, RunService, Step, claim_lost},
    },
};

impl RunService {
    pub(super) async fn issue_ready<T, F>(
        &self,
        runner: &Uuid7,
        admitted: &AdmittedFleetWork,
        policy: ExecutionPolicy<'_>,
        now: Timestamp,
        began: Instant,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, Offered) -> T + Send,
    {
        let Some(issued) = self
            .runs
            .issue_for_revision(
                runner,
                &admitted.acquired,
                Billed {
                    tenant_id: &admitted.billed.tenant_id,
                    posture: admitted.billed.posture.as_str(),
                    provider: &admitted.billed.provider,
                    model: &admitted.billed.model,
                },
                now,
                admitted.installed.issue_revision.as_ref(),
            )
            .await?
        else {
            return Ok(claim_lost(runner, &admitted.acquired));
        };
        if admitted.acquired.kind == crate::run::Kind::Reclaim {
            self.runs
                .publish_restarted(&admitted.acquired, &issued)
                .await;
        }
        self.runs
            .latest()
            .observe(
                &admitted.acquired.fleet_id,
                admitted
                    .installed
                    .issue_revision
                    .as_ref()
                    .map(|proof| &proof.current),
                admitted.acquired.opened.catalogue_version,
            )
            .await;
        af_observability::producers::fleet::runner::run_taken(runner.as_str());
        super::log_issued(
            runner,
            &issued.run_id,
            &admitted.acquired,
            super::queue_wait::issued_at(now, began),
        );
        self.render_offer(runner, admitted, policy, issued, emit)
            .await
    }

    async fn render_offer<T, F>(
        &self,
        runner: &Uuid7,
        admitted: &AdmittedFleetWork,
        policy: ExecutionPolicy<'_>,
        issued: Issued,
        emit: &mut F,
    ) -> Result<Step<Assigned<T>>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, Offered) -> T + Send,
    {
        let turn_resume = if issued.task.is_none() {
            self.runs
                .turn_resume(super::super::turn_checkpoint::TurnRun {
                    runner_id: runner,
                    run_id: &issued.run_id,
                    fencing_token: admitted.acquired.fence.as_u64(),
                })
                .await?
        } else {
            None
        };
        let (history, memory) = future::join(
            history::turns(
                self.thread.as_ref(),
                &admitted.acquired,
                admitted.event_type,
            ),
            self.window_for(&admitted.acquired),
        )
        .await;
        let metadata = metadata(&issued, admitted);
        let body = payload(
            &issued.run_id,
            &admitted.acquired,
            admitted.event_type,
            &admitted.installed,
            policy,
            Context {
                history,
                memory,
                decision: issued.decision,
                task: issued.task,
                turn_resume,
                expires_at: issued.expires_at,
            },
        );
        Ok(Step::Go(Assigned(emit(body, metadata))))
    }
}

fn metadata(issued: &Issued, admitted: &AdmittedFleetWork) -> Offered {
    let acquired = &admitted.acquired;
    Offered {
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
            tenant_id: admitted.billed.tenant_id.clone(),
            turn_id: acquired.turn_id.clone(),
            actor: acquired.actor.clone(),
            posture: admitted.billed.posture.as_str().to_owned(),
            provider: admitted.billed.provider.to_string(),
            model: admitted.billed.model.to_string(),
            fence: acquired.fence,
            run_created_at: issued.created_at,
            turn_created_at: acquired.turn_created_at,
            catalogue_version: Some(acquired.opened.catalogue_version),
        }),
        draft: None,
    }
}
