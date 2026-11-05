//! Durable objective ownership within the existing admission and lease path.
//!
//! The store owns native rows and transitions. Its pool remains the lease
//! store's private pool, and task work never introduces a competing queue.

mod admit;
mod adoption;
mod bindings;
mod checkpoint;
mod continuation;
mod control;
mod decision;
mod dependencies;
mod effects;
mod execution;
mod fetch;
mod github;
mod guard;
mod input_receipt;
mod inputs;
pub(crate) mod lease;
mod lifecycle;
mod list;
mod observe;
mod prepared;
mod read;
mod refusal;
mod report;
mod row;
mod sql;
pub mod storage;
#[cfg(feature = "test-util")]
mod test_util;
mod transaction;
mod types;
pub(crate) mod usage;
mod waits;

use afd_core::id::Uuid7;
use afd_observability::metrics::label::task::{TaskOperation, TaskOutcome};
use tracing::Instrument as _;

use crate::{Result, lease::Leases};

pub(crate) const DECISION_POLICY_PATH: &[&str] = &["config", "x-agentsfleet", "decision_policy"];

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
    TaskAfter, TaskBinding, TaskBindings, TaskControl, TaskLease, TaskPage, TaskScope,
    TaskSnapshot, TaskSummary,
};

pub use lifecycle::{TaskFuture, TaskLifecycle};
pub use report::{CheckpointGuard, PreparedReport, TaskReportOwner, TaskSettlement};
pub(crate) use waits::ReadPermit;
pub use waits::{CompletionScope, VerifiedCompletion, WaitMaintenance};
pub(crate) async fn settle_report(
    connection: &mut sqlx::PgConnection,
    lease: &Uuid7,
    runner: &Uuid7,
    settlement: Option<TaskSettlement<'_>>,
) -> Result<Option<&'static str>> {
    observe::measure(
        TaskOperation::ReportCommit,
        report::apply(connection, lease, runner, settlement),
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
    leases: Leases,
    admissions: afd_admission::Admissions,
    dependencies: TaskDependencies,
}

impl Tasks {
    /// Uses the same database, entropy and lease authority as the fleet plane.
    #[must_use]
    pub const fn new(
        leases: Leases,
        admissions: afd_admission::Admissions,
        dependencies: TaskDependencies,
    ) -> Self {
        Self {
            leases,
            admissions,
            dependencies,
        }
    }

    async fn publish_changed(&self, scope: &TaskScope, task_id: &Uuid7, version: u64) {
        let frame = afd_api_wire::tail::TailFrame::TaskChanged {
            task_id: std::borrow::Cow::Borrowed(task_id.as_str()),
            resource_version: version,
        };
        self.leases
            .streams()
            .publish_frame(scope.fleet_id.as_str(), &frame)
            .await;
    }
}

impl TaskLifecycle for Tasks {
    fn maintain_waits(&self) -> TaskFuture<'_, WaitMaintenance> {
        Box::pin(observe::measure(
            TaskOperation::WaitSweep,
            waits::maintain(self),
            |_| TaskOutcome::Accepted,
        ))
    }
    fn record_github_completion<'a>(
        &'a self,
        completion: &'a VerifiedCompletion,
    ) -> TaskFuture<'a, u64> {
        Box::pin(observe::measure(
            TaskOperation::CompletionMatch,
            waits::complete(self, completion),
            |_| TaskOutcome::Accepted,
        ))
    }
    fn maintain_checkpoints<'a>(
        &'a self,
        cursor: &'a mut CheckpointMaintenanceCursor,
    ) -> TaskFuture<'a, CheckpointMaintenance> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointSweep,
                checkpoint::maintain(self, cursor),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!("checkpoint_maintenance")),
        )
    }
    fn plan_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::CheckpointPlanInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointPlan> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointPlan,
                checkpoint::create(self, lease, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "plan_checkpoint",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn write_checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointPageInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointPage> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointPage,
                checkpoint::write(self, lease, checkpoint, page, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "write_checkpoint_page",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointReadQuery>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointPage> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointPage,
                checkpoint::read(self, lease, checkpoint, page, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "checkpoint_page",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn register_checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointObjectsInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointObjectsPage> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointRegister,
                checkpoint::register(self, lease, checkpoint, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "register_checkpoint_objects",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn checkpoint_uploads<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointUploadsInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointObjectsPage> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointTransfer,
                checkpoint::uploads(self, lease, checkpoint, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "checkpoint_uploads",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn checkpoint_authorizations<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointAuthorizationInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointAuthorizations> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointTransfer,
                checkpoint::authorize(self, lease, checkpoint, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "checkpoint_authorizations",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointReadQuery>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointObjectsPage> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointTransfer,
                checkpoint::list(self, lease, checkpoint, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "checkpoint_objects",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn renew_checkpoint_pin<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        pin: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointPinInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointPin> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointPin,
                checkpoint::renew(self, lease, checkpoint, pin, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "renew_checkpoint_pin",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn commit_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::CheckpointCommitInput>,
    ) -> TaskFuture<'a, afd_wire::task::CheckpointPlan> {
        Box::pin(
            observe::measure(
                TaskOperation::CheckpointCommit,
                checkpoint::publish(self, lease, checkpoint, input),
                |_| TaskOutcome::Accepted,
            )
            .instrument(tracing::info_span!(
                "commit_checkpoint",
                lease_id = lease.lease_id.as_str()
            )),
        )
    }
    fn maintain_effects(&self) -> TaskFuture<'_, EffectMaintenance> {
        Box::pin(observe::measure(
            TaskOperation::EffectSweep,
            effects::maintain(self),
            |_| TaskOutcome::Accepted,
        ))
    }
    fn accept_input<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::TaskInputReceiptInput>,
    ) -> TaskFuture<'a, afd_wire::task::TaskInputReceiptResult> {
        Box::pin(observe::measure(
            TaskOperation::InputReceipt,
            input_receipt::accept(self, lease, input),
            |_| TaskOutcome::Accepted,
        ))
    }
    fn interpret<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::TaskBindingsInput>,
    ) -> TaskFuture<'a, afd_wire::task::TaskBindingsResponse<'static>> {
        Box::pin(bindings::accept(self, lease, input))
    }
    fn publish_effect<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::TaskEffectInput>,
    ) -> TaskFuture<'a, (afd_wire::task::TaskEffect, bool)> {
        Box::pin(observe::measure(
            TaskOperation::EffectPublish,
            effects::publish(self, lease, input),
            |_| TaskOutcome::Accepted,
        ))
    }

    fn effect<'a>(
        &'a self,
        lease: TaskLease<'a>,
        id: &'a Uuid7,
    ) -> TaskFuture<'a, afd_wire::task::TaskEffect> {
        Box::pin(observe::measure(
            TaskOperation::EffectRead,
            effects::one(self, lease, id),
            |_| TaskOutcome::Accepted,
        ))
    }

    fn effects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::TaskEffectsQuery>,
    ) -> TaskFuture<'a, afd_wire::task::TaskEffectsPage> {
        Box::pin(observe::measure(
            TaskOperation::EffectRead,
            effects::list(self, lease, input),
            |_| TaskOutcome::Accepted,
        ))
    }
    fn inputs<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::TaskInputsQuery>,
    ) -> TaskFuture<'a, afd_wire::task::TaskInputsResponse<'static>> {
        Box::pin(observe::measure(
            TaskOperation::Read,
            inputs::read(self, lease, input),
            |_| TaskOutcome::Accepted,
        ))
    }

    fn adopt_goal<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        adoption: &'a TaskAdoption,
    ) -> TaskFuture<'a, TaskSnapshot> {
        let span = tracing::info_span!("task_goal_adoption", task_id = task_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::AdoptGoal,
                adoption::apply(self, scope, task_id, expected_tag, adoption),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
    }

    fn maintain_decisions<'a>(
        &'a self,
        cursor: &'a mut DecisionMaintenanceCursor,
    ) -> TaskFuture<'a, DecisionMaintenance> {
        let span = tracing::info_span!("task_decision_maintenance");
        Box::pin(
            observe::measure(
                TaskOperation::DecisionSweep,
                decision::maintain(self, cursor),
                |swept| {
                    if swept.failed == 0 {
                        TaskOutcome::Accepted
                    } else {
                        TaskOutcome::Failed
                    }
                },
            )
            .instrument(span),
        )
    }
    fn reserve_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<afd_wire::task::DecisionReservationInput>,
    ) -> TaskFuture<'a, ReservedDecision> {
        let span = tracing::info_span!(
            "task_decision_reserve",
            lease_id = lease.lease_id.as_str(),
            source_admission_id = input.source_admission_id.as_str()
        );
        Box::pin(
            observe::measure(
                TaskOperation::DecisionReserve,
                decision::reserve(self, lease, input),
                |result| {
                    if result.issued {
                        TaskOutcome::Accepted
                    } else {
                        TaskOutcome::Replayed
                    }
                },
            )
            .instrument(span),
        )
    }

    fn record_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        call: &'a Uuid7,
        input: &'a garde::Valid<afd_wire::task::DecisionReceiptInput>,
    ) -> TaskFuture<'a, afd_wire::task::DecisionReceiptResult> {
        let span = tracing::info_span!(
            "task_decision_record",
            lease_id = lease.lease_id.as_str(),
            decision_call_id = call.as_str()
        );
        Box::pin(
            observe::measure(
                TaskOperation::DecisionRecord,
                decision::record(self, lease, call, input),
                |result| {
                    if result.replayed {
                        TaskOutcome::Replayed
                    } else {
                        TaskOutcome::Accepted
                    }
                },
            )
            .instrument(span),
        )
    }
    fn list<'a>(
        &'a self,
        scope: &'a TaskScope,
        after: Option<&'a TaskAfter>,
        limit: u32,
    ) -> TaskFuture<'a, TaskPage> {
        let span = tracing::info_span!("task_list", fleet_id = scope.fleet_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::Read,
                list::read(self, scope, after, limit),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
    }

    fn bind<'a>(
        &'a self,
        lease: TaskLease<'a>,
        plan: &'a PreparedPlan,
    ) -> TaskFuture<'a, TaskBindings> {
        let span = tracing::info_span!("task_bind", lease_id = lease.lease_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::Bind,
                admit::bind(self, lease, plan),
                |bindings| {
                    if bindings.replayed {
                        TaskOutcome::Replayed
                    } else {
                        TaskOutcome::Accepted
                    }
                },
            )
            .instrument(span),
        )
    }

    fn read<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
    ) -> TaskFuture<'a, TaskSnapshot> {
        let span = tracing::info_span!("task_read", task_id = task_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::Read,
                read::read(self, scope, task_id),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
    }

    fn control<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        expected_generation: u64,
        control: TaskControl,
    ) -> TaskFuture<'a, TaskSnapshot> {
        let operation = match control {
            TaskControl::Pause => TaskOperation::Pause,
            TaskControl::Resume => TaskOperation::Resume,
            TaskControl::Cancel => TaskOperation::Cancel,
        };
        let span = tracing::info_span!("task_control", task_id = task_id.as_str());
        Box::pin(
            observe::measure(
                operation,
                control::apply(
                    self,
                    scope,
                    task_id,
                    expected_tag,
                    expected_generation,
                    control,
                ),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
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
