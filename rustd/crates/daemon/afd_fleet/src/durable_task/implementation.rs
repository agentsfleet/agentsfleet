//! Task lifecycle entry points retain one operation observation and their original tracing spans.

use super::{
    CheckpointMaintenance, CheckpointMaintenanceCursor, DurableTaskAdoption, DurableTaskAfter,
    DurableTaskControl, DurableTaskFuture as Reply, DurableTaskLifecycle, DurableTaskPage,
    DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, DurableTasks, ReservedDecision,
    adoption, bindings, checkpoint, control, decision, effects, input_receipt, inputs, list,
    observe, read,
};
#[cfg(any(test, feature = "test-util"))]
use super::{DurableTaskBindings, PreparedPlan, admit};
use af_core::id::Uuid7;
use af_observability::metrics::label::durable_task::{DurableTaskOperation, DurableTaskOutcome};
use af_wire::durable_task as wire;
use garde::Valid;

impl DurableTaskLifecycle for DurableTasks {
    fn maintain_checkpoints<'a>(
        &'a self,
        cursor: &'a mut CheckpointMaintenanceCursor,
    ) -> Reply<'a, CheckpointMaintenance> {
        observe::trace(
            DurableTaskOperation::CheckpointSweep,
            checkpoint::maintain(self, cursor),
            tracing::info_span!("checkpoint_maintenance"),
        )
    }
    fn plan_checkpoint<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::CheckpointPlanInput>,
    ) -> Reply<'a, wire::CheckpointPlan> {
        observe::trace(
            DurableTaskOperation::CheckpointPlan,
            checkpoint::create(self, run, input),
            tracing::info_span!("plan_checkpoint", run_id = run.run_id.as_str()),
        )
    }
    fn write_checkpoint_page<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a Valid<wire::CheckpointPageInput>,
    ) -> Reply<'a, wire::CheckpointPage> {
        observe::trace(
            DurableTaskOperation::CheckpointPage,
            checkpoint::write(self, run, checkpoint, page, input),
            tracing::info_span!("write_checkpoint_page", run_id = run.run_id.as_str()),
        )
    }
    fn checkpoint_page<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a Valid<wire::CheckpointReadQuery>,
    ) -> Reply<'a, wire::CheckpointPage> {
        observe::trace(
            DurableTaskOperation::CheckpointPage,
            checkpoint::read(self, run, checkpoint, page, input),
            tracing::info_span!("checkpoint_page", run_id = run.run_id.as_str()),
        )
    }
    fn register_checkpoint_objects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointObjectsInput>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            DurableTaskOperation::CheckpointRegister,
            checkpoint::register(self, run, checkpoint, input),
            tracing::info_span!("register_checkpoint_objects", run_id = run.run_id.as_str()),
        )
    }
    fn checkpoint_uploads<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointUploadsInput>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            DurableTaskOperation::CheckpointTransfer,
            checkpoint::uploads(self, run, checkpoint, input),
            tracing::info_span!("checkpoint_uploads", run_id = run.run_id.as_str()),
        )
    }
    fn checkpoint_authorizations<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointAuthorizationInput>,
    ) -> Reply<'a, wire::CheckpointAuthorizations> {
        observe::trace(
            DurableTaskOperation::CheckpointTransfer,
            checkpoint::authorize(self, run, checkpoint, input),
            tracing::info_span!("checkpoint_authorizations", run_id = run.run_id.as_str()),
        )
    }
    fn checkpoint_objects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointReadQuery>,
    ) -> Reply<'a, wire::CheckpointObjectsPage> {
        observe::trace(
            DurableTaskOperation::CheckpointTransfer,
            checkpoint::list(self, run, checkpoint, input),
            tracing::info_span!("checkpoint_objects", run_id = run.run_id.as_str()),
        )
    }
    fn renew_checkpoint_pin<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        pin: &'a Uuid7,
        input: &'a Valid<wire::CheckpointPinInput>,
    ) -> Reply<'a, wire::CheckpointPin> {
        observe::trace(
            DurableTaskOperation::CheckpointPin,
            checkpoint::renew(self, run, checkpoint, pin, input),
            tracing::info_span!("renew_checkpoint_pin", run_id = run.run_id.as_str()),
        )
    }
    fn commit_checkpoint<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a Valid<wire::CheckpointCommitInput>,
    ) -> Reply<'a, wire::CheckpointPlan> {
        observe::trace(
            DurableTaskOperation::CheckpointCommit,
            checkpoint::publish(self, run, checkpoint, input),
            tracing::info_span!("commit_checkpoint", run_id = run.run_id.as_str()),
        )
    }
    fn accept_input<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DurableTaskInputReceiptInput>,
    ) -> Reply<'a, wire::DurableTaskInputReceiptResult> {
        observe::call(
            DurableTaskOperation::InputReceipt,
            input_receipt::accept(self, run, input),
        )
    }
    fn interpret<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DurableTaskBindingsInput>,
    ) -> Reply<'a, wire::DurableTaskBindingsResponse<'static>> {
        let span = tracing::info_span!("task_bind", run_id = run.run_id.as_str());
        observe::traced(
            DurableTaskOperation::Bind,
            bindings::accept(self, run, input),
            |_bindings| DurableTaskOutcome::Accepted,
            span,
        )
    }
    fn publish_effect<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DurableTaskEffectInput>,
    ) -> Reply<'a, (wire::DurableTaskEffect, bool)> {
        observe::call(
            DurableTaskOperation::EffectPublish,
            effects::publish(self, run, input),
        )
    }

    fn effect<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        id: &'a Uuid7,
    ) -> Reply<'a, wire::DurableTaskEffect> {
        observe::call(
            DurableTaskOperation::EffectRead,
            effects::one(self, run, id),
        )
    }

    fn effects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DurableTaskEffectsQuery>,
    ) -> Reply<'a, wire::DurableTaskEffectsPage> {
        observe::call(
            DurableTaskOperation::EffectRead,
            effects::list(self, run, input),
        )
    }
    fn inputs<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DurableTaskInputsQuery>,
    ) -> Reply<'a, wire::DurableTaskInputsResponse<'static>> {
        observe::call(DurableTaskOperation::Read, inputs::read(self, run, input))
    }

    fn adopt_goal<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
        expected_tag: &'a str,
        adoption: &'a DurableTaskAdoption,
    ) -> Reply<'a, DurableTaskSnapshot> {
        let span = tracing::info_span!(
            "task_goal_adoption",
            durable_task_id = durable_task_id.as_str()
        );
        observe::trace(
            DurableTaskOperation::AdoptGoal,
            adoption::apply(self, scope, durable_task_id, expected_tag, adoption),
            span,
        )
    }

    fn reserve_decision<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a Valid<wire::DecisionReservationInput>,
    ) -> Reply<'a, ReservedDecision> {
        let span = tracing::info_span!(
            "task_decision_reserve",
            run_id = run.run_id.as_str(),
            source_turn_id = input.source_turn_id.as_str()
        );
        observe::traced(
            DurableTaskOperation::DecisionReserve,
            decision::reserve(self, run, input),
            |result| {
                if result.issued {
                    DurableTaskOutcome::Accepted
                } else {
                    DurableTaskOutcome::Replayed
                }
            },
            span,
        )
    }

    fn record_decision<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        call: &'a Uuid7,
        input: &'a Valid<wire::DecisionReceiptInput>,
    ) -> Reply<'a, wire::DecisionReceiptResult> {
        let span = tracing::info_span!(
            "task_decision_record",
            run_id = run.run_id.as_str(),
            decision_call_id = call.as_str()
        );
        observe::traced(
            DurableTaskOperation::DecisionRecord,
            decision::record(self, run, call, input),
            |result| {
                if result.replayed {
                    DurableTaskOutcome::Replayed
                } else {
                    DurableTaskOutcome::Accepted
                }
            },
            span,
        )
    }
    fn list<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        after: Option<&'a DurableTaskAfter>,
        limit: u32,
    ) -> Reply<'a, DurableTaskPage> {
        let span = tracing::info_span!("task_list", fleet_id = scope.fleet_id.as_str());
        observe::trace(
            DurableTaskOperation::Read,
            list::read(self, scope, after, limit),
            span,
        )
    }

    #[cfg(any(test, feature = "test-util"))]
    fn bind<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        plan: &'a PreparedPlan,
    ) -> Reply<'a, DurableTaskBindings> {
        admit::observed_bind(self, run, plan)
    }

    fn read<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
    ) -> Reply<'a, DurableTaskSnapshot> {
        let span = tracing::info_span!("task_read", durable_task_id = durable_task_id.as_str());
        observe::trace(
            DurableTaskOperation::Read,
            read::read(self, scope, durable_task_id),
            span,
        )
    }

    fn control<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
        expected_tag: &'a str,
        expected_generation: u64,
        control: DurableTaskControl,
    ) -> Reply<'a, DurableTaskSnapshot> {
        let operation = match control {
            DurableTaskControl::Pause => DurableTaskOperation::Pause,
            DurableTaskControl::Resume => DurableTaskOperation::Resume,
            DurableTaskControl::Cancel => DurableTaskOperation::Cancel,
        };
        let span = tracing::info_span!("task_control", durable_task_id = durable_task_id.as_str());
        observe::trace(
            operation,
            control::apply(
                self,
                scope,
                durable_task_id,
                expected_tag,
                expected_generation,
                control,
            ),
            span,
        )
    }
}
