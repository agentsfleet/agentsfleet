//! Behavior of the existing native task owner, shared by person and runner APIs.

use super::{
    DecisionMaintenance, DecisionMaintenanceCursor, PreparedPlan, ReservedDecision, TaskAdoption,
    TaskAfter, TaskBindings, TaskControl, TaskLease, TaskPage, TaskScope, TaskSnapshot,
};
use crate::Result;
use af_core::id::Uuid7;
use std::pin::Pin;

/// Object-safe native operation using the owning crate's error shell.
pub type TaskFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Behavior of the authoritative task owner; all writes are fenced natively.
pub trait TaskLifecycle: Send + Sync + std::fmt::Debug {
    /// Observe due native dependencies with bounded concurrency, without an agent lease.
    fn maintain_waits(&self) -> TaskFuture<'_, super::WaitMaintenance>;
    /// Retain verified provider evidence; it can only request an independent native read.
    fn record_github_completion<'a>(
        &'a self,
        completion: &'a super::VerifiedCompletion,
    ) -> TaskFuture<'a, u64>;
    /// Collect private generations and acknowledge bounded remote storage purges.
    fn maintain_checkpoints<'a>(
        &'a self,
        cursor: &'a mut super::CheckpointMaintenanceCursor,
    ) -> TaskFuture<'a, super::CheckpointMaintenance>;
    /// Allocate a fenced capture plan with immutable native page identities.
    fn plan_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::CheckpointPlanInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointPlan>;
    /// Freeze one complete bounded manifest page.
    fn write_checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointPageInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointPage>;
    /// Read one base or overlay page under a current restore pin.
    fn checkpoint_page<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        page: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointReadQuery>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointPage>;
    /// Register at most one bounded batch with one native upload owner per identity.
    fn register_checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointObjectsInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointObjectsPage>;
    /// Freeze exact ciphertext descriptors before authorizing conditional uploads.
    fn checkpoint_uploads<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointUploadsInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointObjectsPage>;
    /// Acquire a restore pin or refresh exact registered transfer material.
    fn checkpoint_authorizations<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointAuthorizationInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointAuthorizations>;
    /// Page restore objects and keys without loading the complete manifest.
    fn checkpoint_objects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointReadQuery>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointObjectsPage>;
    /// Renew one native pin without extending the capture/restore deadline.
    fn renew_checkpoint_pin<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        pin: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointPinInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointPin>;
    /// Publish the saved generation only after native complete-state verification.
    fn commit_checkpoint<'a>(
        &'a self,
        lease: TaskLease<'a>,
        checkpoint: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::CheckpointCommitInput>,
    ) -> TaskFuture<'a, af_wire::task::CheckpointPlan>;
    /// Reconcile issued effects using bounded expiring read-only custody, including after deletion.
    fn maintain_effects(&self) -> TaskFuture<'_, super::EffectMaintenance>;
    /// Acknowledge accepted input application or its own original reply obligation.
    fn accept_input<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::TaskInputReceiptInput>,
    ) -> TaskFuture<'a, af_wire::task::TaskInputReceiptResult>;
    /// Accept complete native routing/association proofs through the existing bind path.
    fn interpret<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::TaskBindingsInput>,
    ) -> TaskFuture<'a, af_wire::task::TaskBindingsResponse<'static>>;
    /// Persist and issue one exact-head publication, or return its immutable replay.
    fn publish_effect<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::TaskEffectInput>,
    ) -> TaskFuture<'a, (af_wire::task::TaskEffect, bool)>;
    /// Read and reconcile an issued publication without repeating its mutation.
    fn effect<'a>(
        &'a self,
        lease: TaskLease<'a>,
        id: &'a Uuid7,
    ) -> TaskFuture<'a, af_wire::task::TaskEffect>;
    /// Recover every effect following a saved checkpoint watermark.
    fn effects<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::TaskEffectsQuery>,
    ) -> TaskFuture<'a, af_wire::task::TaskEffectsPage>;
    /// Read complete pending human input without consuming or reordering it.
    fn inputs<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::TaskInputsQuery>,
    ) -> TaskFuture<'a, af_wire::task::TaskInputsResponse<'static>>;

    /// Adopt reviewed goal criteria without changing target authority or cumulative limits.
    fn adopt_goal<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        adoption: &'a TaskAdoption,
    ) -> TaskFuture<'a, TaskSnapshot>;

    /// Release expired reservations only when native history proves no issuance.
    fn maintain_decisions<'a>(
        &'a self,
        cursor: &'a mut DecisionMaintenanceCursor,
    ) -> TaskFuture<'a, DecisionMaintenance>;
    /// Reserve one physical call; replays cannot issue another credential.
    fn reserve_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        input: &'a garde::Valid<af_wire::task::DecisionReservationInput>,
    ) -> TaskFuture<'a, ReservedDecision>;
    /// Account original-owner usage independently of current task authority.
    fn record_decision<'a>(
        &'a self,
        lease: TaskLease<'a>,
        call: &'a Uuid7,
        input: &'a garde::Valid<af_wire::task::DecisionReceiptInput>,
    ) -> TaskFuture<'a, af_wire::task::DecisionReceiptResult>;
    /// Page current work without loading private objectives or question bodies.
    fn list<'a>(
        &'a self,
        scope: &'a TaskScope,
        after: Option<&'a TaskAfter>,
        limit: u32,
    ) -> TaskFuture<'a, TaskPage>;
    /// Bind a complete prepared interpretation without allocating on replay.
    fn bind<'a>(
        &'a self,
        lease: TaskLease<'a>,
        plan: &'a PreparedPlan,
    ) -> TaskFuture<'a, TaskBindings>;
    /// Read a task only inside the caller's authorized workspace/fleet scope.
    fn read<'a>(&'a self, scope: &'a TaskScope, task_id: &'a Uuid7)
    -> TaskFuture<'a, TaskSnapshot>;
    /// Apply a versioned human lifecycle control without resetting budgets.
    fn control<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_tag: &'a str,
        expected_generation: u64,
        control: TaskControl,
    ) -> TaskFuture<'a, TaskSnapshot>;
}
