//! Freeze the durable offer target and serialize while the policy still borrows its inputs.

use std::time::Instant;

use af_core::id::Uuid7;
use af_wire::{lease::LeasePayload, policy::ExecutionPolicy};
use futures_util::future;
use jiff::Timestamp;

use crate::{
    Result,
    lease::{
        Billed, Issued, Offered, Target,
        answer::{Context, payload},
        history,
        pull::{AdmittedFleetWork, LeaseService, Leased, Step, claim_lost},
    },
};

impl LeaseService {
    pub(super) async fn issue_ready<T, F>(
        &self,
        runner: &Uuid7,
        admitted: &AdmittedFleetWork,
        policy: ExecutionPolicy<'_>,
        now: Timestamp,
        began: Instant,
        emit: &mut F,
    ) -> Result<Step<Leased<T>>>
    where
        T: Send,
        F: for<'a> FnMut(LeasePayload<'a>, Offered) -> T + Send,
    {
        let Some(issued) = self
            .leases
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
        self.leases
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
        af_observability::producers::fleet::runner::lease_taken(runner.as_str());
        super::log_issued(
            runner,
            &issued.lease_id,
            &admitted.acquired,
            super::queue_wait::issued_at(now, began),
        );
        self.render_offer(admitted, policy, issued, emit).await
    }

    async fn render_offer<T, F>(
        &self,
        admitted: &AdmittedFleetWork,
        policy: ExecutionPolicy<'_>,
        issued: Issued,
        emit: &mut F,
    ) -> Result<Step<Leased<T>>>
    where
        T: Send,
        F: for<'a> FnMut(LeasePayload<'a>, Offered) -> T + Send,
    {
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
            &issued.lease_id,
            &admitted.acquired,
            admitted.event_type,
            &admitted.installed,
            policy,
            Context {
                history,
                memory,
                decision: issued.decision,
                task: issued.task,
                expires_at: Some(issued.expires_at),
                execution_deadline: Some(issued.execution_deadline),
            },
        );
        Ok(Step::Go(Leased(emit(body, metadata))))
    }
}

fn metadata(issued: &Issued, admitted: &AdmittedFleetWork) -> Offered {
    let acquired = &admitted.acquired;
    Offered {
        lease_id: issued.lease_id.clone(),
        expires_at: issued.expires_at,
        target: Some(Target {
            fleet_id: acquired.fleet_id.clone(),
            event_id: acquired.event_id.clone(),
            lease_created_at: issued.created_at,
            event_created_at: acquired.event_created_at,
            timing_eligible: true,
            fence: acquired.fence.as_i64(),
        }),
        // The columns `select_lease_for_report.sql` reads, as this issue
        // wrote them. A durable task's lease keeps none: it renews on the
        // per-lease path, which reads its task's authority.
        reported: issued.task.is_none().then(|| crate::lease::Reported {
            fleet_id: acquired.fleet_id.clone(),
            workspace_id: acquired.workspace_id.clone(),
            tenant_id: admitted.billed.tenant_id.clone(),
            event_id: acquired.event_id.clone(),
            actor: acquired.actor.clone(),
            posture: admitted.billed.posture.as_str().to_owned(),
            provider: admitted.billed.provider.to_string(),
            model: admitted.billed.model.to_string(),
            fence: acquired.fence,
            lease_created_at: issued.created_at,
            event_created_at: acquired.event_created_at,
            catalogue_version: Some(acquired.opened.catalogue_version),
        }),
    }
}
