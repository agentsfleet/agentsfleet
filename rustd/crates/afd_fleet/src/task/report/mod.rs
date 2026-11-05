//! Task state joins the existing report transaction before its slot is released.

mod checkpoint;
mod prepare;
mod settle;

#[cfg(feature = "test-util")]
pub use checkpoint::CheckpointGuard;

use super::{
    TaskFuture, TaskLease, TaskScope, TaskSnapshot, Tasks,
    continuation::Continuation,
    github::{BoundPr, status::Snapshot},
};
use afd_core::id::Uuid7;
use afd_wire::{report::ReportRequest, task::TaskReport};

/// Native report preparation; HTTP cannot construct a semantic permit.
pub trait TaskReportOwner: std::fmt::Debug + Send + Sync {
    /// Read recorded decisions and current dependency evidence outside the report transaction.
    fn prepare_report<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a ReportRequest<'a>,
    ) -> TaskFuture<'a, Option<PreparedReport>>;
}

/// Immutable native preparation with no credential or model-selected authority.
#[derive(Debug)]
pub struct PreparedReport {
    tasks: Tasks,
    lease_id: Uuid7,
    runner_id: String,
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
        lease: TaskLease<'a>,
        input: &'a ReportRequest<'a>,
    ) -> TaskFuture<'a, Option<PreparedReport>> {
        if input.task.is_none() {
            return Box::pin(async { Ok(None) });
        }
        Box::pin(super::observe::measure(
            afd_observability::metrics::label::task::TaskOperation::ReportPrepare,
            prepare::prepare(self, lease, input),
            |prepared| {
                if prepared.is_some() {
                    afd_observability::metrics::label::task::TaskOutcome::Accepted
                } else {
                    afd_observability::metrics::label::task::TaskOutcome::Unavailable
                }
            },
        ))
    }
}

/// One report's prepared admission borrows its native continuation until commit.
#[derive(Debug)]
pub struct TaskSettlement<'a> {
    plan: &'a PreparedReport,
    admission: Option<afd_admission::PreparedAdmission<'a>>,
}

impl PreparedReport {
    /// Read admission capacity before the report takes host/slot/task locks.
    pub(crate) async fn settlement(&self) -> crate::Result<TaskSettlement<'_>> {
        let admission = match &self.wake {
            Some(wake) => Some(self.tasks.admissions.prepare(wake.admission()).await?),
            None => None,
        };
        Ok(TaskSettlement {
            plan: self,
            admission,
        })
    }
    pub(crate) async fn announce(&self) {
        let Ok(id) = super::guard::id(&self.snapshot.context.task_id, "task_id") else {
            return;
        };
        if let Ok(snapshot) = super::read::read(&self.tasks, &self.scope, &id).await {
            if snapshot.context.state == afd_wire::task::TaskState::Ready
                && Some(snapshot.context.generation)
                    == self.snapshot.context.generation.checked_add(1)
            {
                if let Some(wake) = &self.wake {
                    wake.confirm(&self.tasks).await;
                }
            }
            self.tasks
                .publish_changed(&self.scope, &id, snapshot.resource_version)
                .await;
        }
    }
}

pub(crate) use settle::apply;
