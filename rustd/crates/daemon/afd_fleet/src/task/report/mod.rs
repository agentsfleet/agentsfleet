//! Task state joins the existing report transaction before its thread is released.

mod checkpoint;
mod prepare;
mod settle;

pub use checkpoint::CheckpointGuard;

use super::{
    TaskFuture, TaskRun, TaskScope, TaskSnapshot, Tasks,
    continuation::Continuation,
    github::{BoundPr, status::Snapshot},
};
use af_core::id::Uuid7;
use af_wire::{report::ReportRequest, task::TaskReport};

/// Native report preparation; HTTP cannot construct a semantic permit.
pub trait TaskReportOwner: std::fmt::Debug + Send + Sync {
    /// Read recorded decisions and current dependency evidence outside the report transaction.
    fn prepare_report<'a>(
        &'a self,
        run: TaskRun<'a>,
        input: &'a ReportRequest<'a>,
    ) -> TaskFuture<'a, Option<PreparedReport>>;
}

/// Immutable native preparation with no credential or model-selected authority.
#[derive(Debug)]
pub struct PreparedReport {
    tasks: Tasks,
    run_id: Uuid7,
    runner_id: Uuid7,
    scope: TaskScope,
    snapshot: TaskSnapshot,
    input: TaskReport,
    target: Option<BoundPr>,
    dependency: Option<Snapshot>,
    wake: Option<Continuation>,
}

impl TaskReportOwner for Tasks {
    fn prepare_report<'a>(
        &'a self,
        run: TaskRun<'a>,
        input: &'a ReportRequest<'a>,
    ) -> TaskFuture<'a, Option<PreparedReport>> {
        if input.task.is_none() {
            return Box::pin(async { Ok(None) });
        }
        Box::pin(super::observe::measure(
            af_observability::metrics::label::task::TaskOperation::ReportPrepare,
            prepare::prepare(self, run, input),
            |prepared| {
                if prepared.is_some() {
                    af_observability::metrics::label::task::TaskOutcome::Accepted
                } else {
                    af_observability::metrics::label::task::TaskOutcome::Unavailable
                }
            },
        ))
    }
}

/// One report's prepared turn borrows its native continuation until commit.
#[derive(Debug)]
pub struct TaskSettlement<'a> {
    plan: &'a PreparedReport,
    turn: Option<afd_turns::PreparedTurn<'a>>,
}

impl PreparedReport {
    /// Prepare the continuation turn before the report takes host/thread/task locks.
    pub(crate) async fn settlement(&self) -> crate::Result<TaskSettlement<'_>> {
        let turn = match &self.wake {
            Some(wake) => Some(self.tasks.turns.prepare(wake.turn()).await?),
            None => None,
        };
        Ok(TaskSettlement { plan: self, turn })
    }
    pub(crate) async fn announce(&self) {
        let Ok(id) = super::guard::id(&self.snapshot.context.task_id, "task_id") else {
            return;
        };
        if let Ok(snapshot) = super::read::read(&self.tasks, &self.scope, &id).await {
            if snapshot.context.state == af_wire::task::TaskState::Ready
                && Some(snapshot.context.generation)
                    == self.snapshot.context.generation.checked_add(1)
                && let Some(wake) = &self.wake
            {
                wake.confirm(&self.tasks).await;
            }
            self.tasks
                .publish_changed(&self.scope, &id, snapshot.resource_version)
                .await;
        }
    }
}

pub(crate) use settle::apply;
