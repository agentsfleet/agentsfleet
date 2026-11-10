//! Task lifecycle entry points retain one operation observation and their original tracing spans.

use super::{
    CheckpointMaintenance, CheckpointMaintenanceCursor, DecisionMaintenance,
    DecisionMaintenanceCursor, EffectMaintenance, PreparedPlan, ReservedDecision, TaskAdoption,
    TaskAfter, TaskBindings, TaskControl, TaskFuture as Reply, TaskLease, TaskLifecycle, TaskPage,
    TaskScope, TaskSnapshot, Tasks, VerifiedCompletion, WaitMaintenance, admit, adoption, bindings,
    checkpoint, control, decision, effects, input_receipt, inputs, list, observe, read, waits,
};
use af_core::id::Uuid7;
use af_observability::metrics::label::task::{TaskOperation, TaskOutcome};
use af_wire::task as wire;
use garde::Valid;

impl TaskLifecycle for Tasks {
    fn maintain_waits(&self) -> Reply<'_, WaitMaintenance> {
        observe::call(TaskOperation::WaitSweep, waits::maintain(self))
    }
    fn record_github_completion<'a>(
        &'a self,
        completion: &'a VerifiedCompletion,
    ) -> Reply<'a, u64> {
        observe::call(
            TaskOperation::CompletionMatch,
            waits::complete(self, completion),
        )
    }
    fn maintain_checkpoints<'a>(
        &'a self,
        cursor: &'a mut CheckpointMaintenanceCursor,
    ) -> Reply<'a, CheckpointMaintenance> {
        observe::trace(
            TaskOperation::CheckpointSweep,
            checkpoint::maintain(self, cursor),
            tracing::info_span!("checkpoint_maintenance"),
        )
    }
    fn plan_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::CheckpointPlanInput>,
    ) -> Reply<'a, wire::CheckpointPlan> {
        observe::trace(
            TaskOperation::CheckpointPlan,
            checkpoint::create(self, lease, input),
            tracing::info_span!("plan_checkpoint", lease_id = lease.lease_id.as_str()),
        )
    }
    fn write_checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a Valid<wire::CheckpointPageInput>,
    ) -> Reply<'a, wire::CheckpointPage> {
        observe::trace(
            TaskOperation::CheckpointPage,
            checkpoint::write(self, lease, checkpoint, page, input),
            tracing::info_span!("write_checkpoint_page", lease_id = lease.lease_id.as_str()),
        )
    }
    fn checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a Valid<wire::CheckpointReadQuery>,
    ) -> Reply<'a, wire::CheckpointPage> {
        observe::trace(
            TaskOperation::CheckpointPage,
            checkpoint::read(self, lease, checkpoint, page, input),
            tracing::info_span!("checkpoint_page", lease_id = lease.lease_id.as_str()),
        )
    }
    fn register_checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointObjectsInput>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            TaskOperation::CheckpointRegister,
            checkpoint::register(self, lease, checkpoint, input),
            tracing::info_span!(
                "register_checkpoint_objects",
                lease_id = lease.lease_id.as_str()
            ),
        )
    }
    fn checkpoint_uploads<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointUploadsInput>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            TaskOperation::CheckpointTransfer,
            checkpoint::uploads(self, lease, checkpoint, input),
            tracing::info_span!("checkpoint_uploads", lease_id = lease.lease_id.as_str()),
        )
    }
    fn checkpoint_authorizations<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointAuthorizationInput>,
    ) -> Reply<'a, wire::CheckpointAuthorizations> {
        observe::trace(
            TaskOperation::CheckpointTransfer,
            checkpoint::authorize(self, lease, checkpoint, input),
            tracing::info_span!(
                "checkpoint_authorizations",
                lease_id = lease.lease_id.as_str()
            ),
        )
    }
    fn checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointReadQuery>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            TaskOperation::CheckpointTransfer,
            checkpoint::list(self, lease, checkpoint, input),
            tracing::info_span!("checkpoint_objects", lease_id = lease.lease_id.as_str()),
        )
    }
    fn renew_checkpoint_pin<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        pin: &'a Uuid7,
        input: &'a Valid<wire::CheckpointPinInput>,
    ) -> Reply<'a, wire::CheckpointPin> {
        observe::trace(
            TaskOperation::CheckpointPin,
            checkpoint::renew(self, lease, checkpoint, pin, input),
            tracing::info_span!("renew_checkpoint_pin", lease_id = lease.lease_id.as_str()),
        )
    }
    fn commit_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointCommitInput>,
    ) -> Reply<'a, wire::CheckpointPlan> {
        observe::trace(
            TaskOperation::CheckpointCommit,
            checkpoint::publish(self, lease, checkpoint, input),
            tracing::info_span!("commit_checkpoint", lease_id = lease.lease_id.as_str()),
        )
    }
    fn maintain_effects(&self) -> Reply<'_, EffectMaintenance> {
        observe::call(TaskOperation::EffectSweep, effects::maintain(self))
    }
    fn accept_input<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::TaskInputReceiptInput>,
    ) -> Reply<'a, wire::TaskInputReceiptResult> {
        observe::call(
            TaskOperation::InputReceipt,
            input_receipt::accept(self, lease, input),
        )
    }
    fn interpret<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::TaskBindingsInput>,
    ) -> Reply<'a, wire::TaskBindingsResponse<'static>> {
        Box::pin(bindings::accept(self, lease, input))
    }
    fn publish_effect<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::TaskEffectInput>,
    ) -> Reply<'a, (wire::TaskEffect, bool)> {
        observe::call(
            TaskOperation::EffectPublish,
            effects::publish(self, lease, input),
        )
    }

    fn effect<'a>(&'a self, lease: TaskLease<'a>, id: &'a Uuid7) -> Reply<'a, wire::TaskEffect> {
        observe::call(TaskOperation::EffectRead, effects::one(self, lease, id))
    }

    fn effects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::TaskEffectsQuery>,
    ) -> Reply<'a, wire::TaskEffectsPage> {
        observe::call(TaskOperation::EffectRead, effects::list(self, lease, input))
    }
    fn inputs<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::TaskInputsQuery>,
    ) -> Reply<'a, wire::TaskInputsResponse<'static>> {
        observe::call(TaskOperation::Read, inputs::read(self, lease, input))
    }

    fn adopt_goal<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        adoption: &'a TaskAdoption,
    ) -> Reply<'a, TaskSnapshot> {
        let span = tracing::info_span!("task_goal_adoption", task_id = task_id.as_str());
        observe::trace(
            TaskOperation::AdoptGoal,
            adoption::apply(self, scope, task_id, expected_tag, adoption),
            span,
        )
    }

    fn maintain_decisions<'a>(
        &'a self,
        cursor: &'a mut DecisionMaintenanceCursor,
    ) -> Reply<'a, DecisionMaintenance> {
        let span = tracing::info_span!("task_decision_maintenance");
        observe::traced(
            TaskOperation::DecisionSweep,
            decision::maintain(self, cursor),
            |swept| {
                if swept.failed == 0 {
                    TaskOutcome::Accepted
                } else {
                    TaskOutcome::Failed
                }
            },
            span,
        )
    }
    fn reserve_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a Valid<wire::DecisionReservationInput>,
    ) -> Reply<'a, ReservedDecision> {
        let span = tracing::info_span!(
            "task_decision_reserve",
            lease_id = lease.lease_id.as_str(),
            source_admission_id = input.source_admission_id.as_str()
        );
        observe::traced(
            TaskOperation::DecisionReserve,
            decision::reserve(self, lease, input),
            |result| {
                if result.issued {
                    TaskOutcome::Accepted
                } else {
                    TaskOutcome::Replayed
                }
            },
            span,
        )
    }

    fn record_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        call: &'a Uuid7,
        input: &'a Valid<wire::DecisionReceiptInput>,
    ) -> Reply<'a, wire::DecisionReceiptResult> {
        let span = tracing::info_span!(
            "task_decision_record",
            lease_id = lease.lease_id.as_str(),
            decision_call_id = call.as_str()
        );
        observe::traced(
            TaskOperation::DecisionRecord,
            decision::record(self, lease, call, input),
            |result| {
                if result.replayed {
                    TaskOutcome::Replayed
                } else {
                    TaskOutcome::Accepted
                }
            },
            span,
        )
    }
    fn list<'a>(
        &'a self,
        scope: &'a TaskScope,
        after: Option<&'a TaskAfter>,
        limit: u32,
    ) -> Reply<'a, TaskPage> {
        let span = tracing::info_span!("task_list", fleet_id = scope.fleet_id.as_str());
        observe::trace(
            TaskOperation::Read,
            list::read(self, scope, after, limit),
            span,
        )
    }

    fn bind<'a>(&'a self, lease: TaskLease<'a>, plan: &'a PreparedPlan) -> Reply<'a, TaskBindings> {
        let span = tracing::info_span!("task_bind", lease_id = lease.lease_id.as_str());
        observe::traced(
            TaskOperation::Bind,
            admit::bind(self, lease, plan),
            |bindings| {
                if bindings.replayed {
                    TaskOutcome::Replayed
                } else {
                    TaskOutcome::Accepted
                }
            },
            span,
        )
    }

    fn read<'a>(&'a self, scope: &'a TaskScope, task_id: &'a Uuid7) -> Reply<'a, TaskSnapshot> {
        let span = tracing::info_span!("task_read", task_id = task_id.as_str());
        observe::trace(TaskOperation::Read, read::read(self, scope, task_id), span)
    }

    fn control<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        expected_generation: u64,
        control: TaskControl,
    ) -> Reply<'a, TaskSnapshot> {
        let operation = match control {
            TaskControl::Pause => TaskOperation::Pause,
            TaskControl::Resume => TaskOperation::Resume,
            TaskControl::Cancel => TaskOperation::Cancel,
        };
        let span = tracing::info_span!("task_control", task_id = task_id.as_str());
        observe::trace(
            operation,
            control::apply(
                self,
                scope,
                task_id,
                expected_tag,
                expected_generation,
                control,
            ),
            span,
        )
    }
}
