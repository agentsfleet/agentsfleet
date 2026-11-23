//! Behavior of the existing native task owner, shared by person and runner APIs.

use super::{
    DurableTaskAdoption, DurableTaskAfter, DurableTaskControl, DurableTaskPage, DurableTaskRun,
    DurableTaskScope, DurableTaskSnapshot, ReservedDecision,
};
#[cfg(any(test, feature = "test-util"))]
use super::{DurableTaskBindings, PreparedPlan};
use crate::Result;
use af_core::id::Uuid7;
use std::pin::Pin;

/// Object-safe native operation using the owning crate's error shell.
pub type DurableTaskFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Behavior of the authoritative task owner; all writes are fenced natively.
pub trait DurableTaskLifecycle: Send + Sync + std::fmt::Debug {
    /// Collect private generations and acknowledge bounded remote storage purges.
    fn maintain_checkpoints<'a>(
        &'a self,
        cursor: &'a mut super::CheckpointMaintenanceCursor,
    ) -> DurableTaskFuture<'a, super::CheckpointMaintenance>;
    /// Allocate a fenced capture plan with immutable native page identities.
    fn plan_checkpoint<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointPlanInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointPlan>;
    /// Freeze one complete bounded manifest page.
    fn write_checkpoint_page<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointPageInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointPage>;
    /// Read one base or overlay page under a current restore pin.
    fn checkpoint_page<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointReadQuery>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointPage>;
    /// Register at most one bounded batch with one native upload owner per identity.
    fn register_checkpoint_objects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointObjectsInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointObjectsPage>;
    /// Freeze exact ciphertext descriptors before authorizing conditional uploads.
    fn checkpoint_uploads<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointUploadsInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointObjectsPage>;
    /// Acquire a restore pin or refresh exact registered transfer material.
    fn checkpoint_authorizations<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointAuthorizationInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointAuthorizations>;
    /// Page restore objects and keys without loading the complete manifest.
    fn checkpoint_objects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointReadQuery>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointObjectsPage>;
    /// Renew one native pin without extending the capture/restore deadline.
    fn renew_checkpoint_pin<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        pin: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointPinInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointPin>;
    /// Publish the saved generation only after native complete-state verification.
    fn commit_checkpoint<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::CheckpointCommitInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::CheckpointPlan>;
    /// Acknowledge accepted input application or its own original reply delivery.
    fn accept_input<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DurableTaskInputReceiptInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DurableTaskInputReceiptResult>;
    /// Accept complete native routing/association proofs through the existing bind path.
    fn interpret<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DurableTaskBindingsInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DurableTaskBindingsResponse<'static>>;
    /// Persist and issue one exact-head publication, or return its immutable replay.
    fn publish_effect<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DurableTaskEffectInput>,
    ) -> DurableTaskFuture<'a, (af_wire::durable_task::DurableTaskEffect, bool)>;
    /// Read and reconcile an issued publication without repeating its mutation.
    fn effect<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        id: &'a Uuid7,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DurableTaskEffect>;
    /// Recover every effect following a saved checkpoint watermark.
    fn effects<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DurableTaskEffectsQuery>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DurableTaskEffectsPage>;
    /// Read complete pending human input without consuming or reordering it.
    fn inputs<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DurableTaskInputsQuery>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DurableTaskInputsResponse<'static>>;

    /// Adopt reviewed goal criteria without changing target authority or cumulative limits.
    fn adopt_goal<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
        expected_tag: &'a str,
        adoption: &'a DurableTaskAdoption,
    ) -> DurableTaskFuture<'a, DurableTaskSnapshot>;

    /// Reserve one physical call; replays cannot issue another credential.
    fn reserve_decision<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        input: &'a garde::Valid<af_wire::durable_task::DecisionReservationInput>,
    ) -> DurableTaskFuture<'a, ReservedDecision>;
    /// Account original-owner usage independently of current task authority.
    fn record_decision<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        call: &'a Uuid7,
        input: &'a garde::Valid<af_wire::durable_task::DecisionReceiptInput>,
    ) -> DurableTaskFuture<'a, af_wire::durable_task::DecisionReceiptResult>;
    /// Page current work without loading private objectives or question bodies.
    fn list<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        after: Option<&'a DurableTaskAfter>,
        limit: u32,
    ) -> DurableTaskFuture<'a, DurableTaskPage>;
    /// Bind a complete prepared interpretation without allocating on replay.
    ///
    /// A test seam: the plan is built by the caller, so it can skip the
    /// authorization that `interpret` proves before it binds. Production reaches
    /// the same binding only through `interpret`, so this method is not compiled
    /// into a build without `test-util`.
    #[cfg(any(test, feature = "test-util"))]
    fn bind<'a>(
        &'a self,
        run: DurableTaskRun<'a>,
        plan: &'a PreparedPlan,
    ) -> DurableTaskFuture<'a, DurableTaskBindings>;
    /// Read a task only inside the caller's authorized workspace/fleet scope.
    fn read<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
    ) -> DurableTaskFuture<'a, DurableTaskSnapshot>;
    /// Apply a versioned human lifecycle control without resetting budgets.
    fn control<'a>(
        &'a self,
        scope: &'a DurableTaskScope,
        durable_task_id: &'a Uuid7,
        expected_tag: &'a str,
        expected_generation: u64,
        control: DurableTaskControl,
    ) -> DurableTaskFuture<'a, DurableTaskSnapshot>;
}
