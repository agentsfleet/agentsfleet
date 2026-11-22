//! Task state joins the existing report transaction before its thread is released.

mod checkpoint;
mod prepare;
mod settle;

pub use checkpoint::CheckpointGuard;

use super::{
    DurableTaskFuture, DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, DurableTasks,
    continuation::Continuation,
    github::{BoundPr, status::Snapshot},
};
use af_core::id::Uuid7;
use af_wire::{durable_task::DurableTaskReport, report::ReportRequest};

/// Native report preparation; HTTP cannot construct a semantic permit.
pub trait DurableTaskReportOwner: std::fmt::Debug + Send + Sync {
    /// Read recorded decisions and current dependency evidence outside the report transaction.
    fn prepare_report<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a ReportRequest<'a>,
    ) -> DurableTaskFuture<'a, Option<PreparedReport>>;
}

/// Immutable native preparation with no credential or model-selected authority.
#[derive(Debug)]
pub struct PreparedReport {
    tasks: DurableTasks,
    run_id: Uuid7,
    runner_id: Uuid7,
    durable_task_id: Uuid7,
    scope: DurableTaskScope,
    snapshot: DurableTaskSnapshot,
    input: DurableTaskReport,
    target: Option<BoundPr>,
    dependency: Option<Snapshot>,
    wake: Option<Continuation>,
}

impl DurableTaskReportOwner for DurableTasks {
    fn prepare_report<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a ReportRequest<'a>,
    ) -> DurableTaskFuture<'a, Option<PreparedReport>> {
        let Some(task) = &input.durable_task else {
            return Box::pin(async { Ok(None) });
        };
        Box::pin(super::observe::measure(
            af_observability::metrics::label::durable_task::DurableTaskOperation::ReportPrepare,
            prepare::prepare(self, run, task),
            |prepared| {
                if prepared.is_some() {
                    af_observability::metrics::label::durable_task::DurableTaskOutcome::Accepted
                } else {
                    af_observability::metrics::label::durable_task::DurableTaskOutcome::Unavailable
                }
            },
        ))
    }
}

/// One report's prepared turn borrows its native continuation until commit.
#[derive(Debug)]
pub struct DurableTaskSettlement<'a> {
    plan: &'a PreparedReport,
    turn: Option<afd_turns::PreparedTurn<'a>>,
}

impl PreparedReport {
    /// Prepare the continuation turn before the report takes host/thread/task locks.
    pub(crate) async fn settlement(&self) -> crate::Result<DurableTaskSettlement<'_>> {
        let turn = match &self.wake {
            Some(wake) => Some(self.tasks.turns.prepare(wake.turn()).await?),
            None => None,
        };
        Ok(DurableTaskSettlement { plan: self, turn })
    }
    pub(crate) async fn announce(&self) {
        let id = &self.durable_task_id;
        if let Ok(snapshot) = super::read::read(&self.tasks, &self.scope, id).await {
            if snapshot.context.state == af_wire::durable_task::DurableTaskState::Ready
                && Some(snapshot.context.generation)
                    == self.snapshot.context.generation.checked_add(1)
                && let Some(wake) = &self.wake
            {
                wake.confirm(&self.tasks).await;
            }
            self.tasks
                .publish_changed(&self.scope, id, snapshot.resource_version)
                .await;
        }
    }
}

pub(crate) use settle::apply;
