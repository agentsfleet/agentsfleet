//! Durable objective ownership within the existing admission and run path.
//!
//! The store owns native rows and transitions. Its pool remains the run
//! store's private pool, and task work never introduces a competing queue.

mod admit;
mod adoption;
mod bindings;
pub(crate) mod checkpoint;
mod continuation;
mod control;
mod decision;
mod dependencies;
mod effects;
mod execution;
mod fetch;
mod github;
mod guard;
mod implementation;
mod input_receipt;
mod inputs;
mod lifecycle;
mod list;
mod observe;
pub mod policy;
mod prepared;
mod read;
mod refusal;
mod report;
mod row;
pub(crate) mod run;
mod sql;
pub mod storage;
#[cfg(feature = "test-util")]
mod test_util;
mod transaction;
mod types;
pub(crate) mod usage;
mod waits;

use af_core::id::Uuid7;
use af_observability::metrics::label::task::{TaskOperation, TaskOutcome};

use crate::{Result, run::Runs};

/// Canonical setup lookup path shared by task guards, queue queries and their measurements.
pub const DECISION_POLICY_PATH: &[&str] = &["config", "x-agentsfleet", "decision_policy"];

pub use decision::ReservedDecision;
#[cfg(feature = "test-util")]
pub use decision::{
    DECISION_BUDGET_STATEMENT, EXPIRED_DECISION_CALLS_STATEMENT, LOCKED_DECISION_CALL_STATEMENT,
};
pub use dependencies::TaskDependencies;
pub use github::Github;
use prepared::PreparedItem;
pub use refusal::TaskRefusal;
pub use types::{
    CheckpointMaintenance, CheckpointMaintenanceCursor, DecisionMaintenance,
    DecisionMaintenanceCursor, EffectMaintenance, PreparedPlan, PreparedTask, TaskAdoption,
    TaskAfter, TaskBinding, TaskBindings, TaskControl, TaskPage, TaskRun, TaskScope, TaskSnapshot,
    TaskSummary,
};

pub use lifecycle::{TaskFuture, TaskLifecycle};
pub use report::{CheckpointGuard, PreparedReport, TaskReportOwner, TaskSettlement};
pub(crate) use waits::ReadPermit;
pub use waits::{CompletionScope, VerifiedCompletion, WaitMaintenance};
pub(crate) async fn settle_report(
    connection: &mut sqlx::PgConnection,
    run: &Uuid7,
    runner: &Uuid7,
    settlement: Option<TaskSettlement<'_>>,
) -> Result<Option<&'static str>> {
    observe::measure(
        TaskOperation::ReportCommit,
        report::apply(connection, run, runner, settlement),
        |notice| {
            if notice.is_none() {
                TaskOutcome::Accepted
            } else {
                TaskOutcome::Unavailable
            }
        },
    )
    .await
}

/// Task data and native dependencies; behavior is implemented by the trait.
#[derive(Debug, Clone)]
pub struct Tasks {
    runs: Runs,
    admissions: afd_turns::Turns,
    dependencies: TaskDependencies,
}

impl Tasks {
    /// Uses the same database, entropy and run authority as the fleet plane.
    #[must_use]
    pub const fn new(
        runs: Runs,
        admissions: afd_turns::Turns,
        dependencies: TaskDependencies,
    ) -> Self {
        Self {
            runs,
            admissions,
            dependencies,
        }
    }

    async fn publish_changed(&self, scope: &TaskScope, task_id: &Uuid7, version: u64) {
        let frame = afd_api_wire::tail::TailFrame::TaskChanged {
            task_id: std::borrow::Cow::Borrowed(task_id.as_str()),
            resource_version: version,
        };
        let subject = afd_bus::Subject::ThreadItems(scope.fleet_id.clone());
        let publication = self.runs.streams().publish_frame(&subject, &frame).await;
        afd_bus::observe_notification(&subject, publication);
    }
}

/// Actual parent-lock statement for API-role migration and contention proofs.
#[cfg(feature = "test-util")]
pub const TASK_SCOPE_LOCK_STATEMENT: &str = sql::LOCK_SCOPE;
/// Actual checkpoint parent-lock statement for API-role migration proofs.
#[cfg(feature = "test-util")]
pub const CHECKPOINT_WORKSPACE_LOCK_STATEMENT: &str = sql::LOCK_WORKSPACE;
/// Actual bounded receipt collection statement for API-role migration proofs.
#[cfg(feature = "test-util")]
pub const TASK_COMPLETION_COLLECTION_STATEMENT: &str = sql::COLLECT_COMPLETIONS;
