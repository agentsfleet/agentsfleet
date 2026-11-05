//! Native task values, constructed after wire validation and target preflight.

use std::ops::Range;

use afd_core::id::Uuid7;
use afd_wire::task::{TaskContext, TaskLimits, TaskState};

/// Bounded recovery of reservations for calls whose credentials were never issued.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecisionMaintenance {
    /// Expired unissued candidates examined by this pass.
    pub scanned: u64,
    /// Reservations proved unsent and released through native accounting.
    pub expired: u64,
    /// Candidates deferred because their existing financial locks were busy.
    pub deferred: u64,
    /// Candidate failures; an already-committed expiry remains counted above.
    pub failed: u64,
    /// Largest observed delay beyond the original issuance deadline.
    pub oldest_overdue_ms: u64,
}

/// Native exact-effect evidence reads never create another publication or agent lease.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectMaintenance {
    /// Retained read capabilities examined by this pass.
    pub scanned: u64,
    /// Exact marker, parent and tree confirmed by GitHub.
    pub confirmed: u64,
    /// Absence retained as uncertainty.
    pub unknown: u64,
    /// Expired encrypted read capabilities erased without a false refusal.
    pub expired: u64,
    /// Deferred read/record failures.
    pub failed: u64,
}

/// Bounded private retention and acknowledged storage cleanup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckpointMaintenance {
    /// Obsolete private checkpoint generations removed.
    pub owners: u64,
    /// Unreferenced retained bases removed.
    pub bases: u64,
    /// Unreferenced object keys erased into durable purge custody.
    pub objects: u64,
    /// Remote deletions acknowledged and quota released once.
    pub purged: u64,
    /// Remote cleanup attempts deferred for later reconciliation.
    pub deferred: u64,
}

/// Local fairness cursor; it carries no task authority or queued work.
#[derive(Debug, Default)]
pub struct CheckpointMaintenanceCursor {
    pub(super) after: Option<(jiff::Timestamp, String)>,
    pub(super) cutoff: Option<jiff::Timestamp>,
}

/// Local scan progress, retained by the existing supervised billing loop.
///
/// This carries no task authority. Advancing before an attempt allows the next
/// pass to progress after cancellation, malformed rows or lock contention.
#[derive(Debug, Default)]
pub struct DecisionMaintenanceCursor {
    pub(super) after: Option<(jiff::Timestamp, String)>,
    pub(super) cutoff: Option<jiff::Timestamp>,
}

/// Workspace ownership already established by the authenticated API boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskScope {
    /// Owning workspace.
    pub workspace_id: Uuid7,
    /// Owning fleet.
    pub fleet_id: Uuid7,
}

/// A runner's current attempt; the task service derives its workspace in SQL.
#[derive(Debug, Clone, Copy)]
pub struct TaskLease<'a> {
    /// Authenticated runner identity, never a body field.
    pub runner_id: &'a Uuid7,
    /// Lease identity supplied by the path.
    pub lease_id: &'a Uuid7,
    /// Native fencing token issued with that lease.
    pub fencing_token: u64,
}

/// An admitted objective after input validation and native target preflight.
///
/// Fields stay private to this owner. Raw model output cannot construct a
/// prepared binding through the public native service interface.
#[derive(Debug, Clone)]
pub struct PreparedTask {
    pub(super) source_span: Range<i32>,
    pub(super) title: String,
    pub(super) objective: String,
    pub(super) questions: Vec<afd_wire::decision::NamedQuestion>,
    pub(super) goal_digest: String,
    pub(super) target_json: String,
    pub(super) limits: TaskLimits,
}

/// Complete immutable interpretation; partial item acceptance is forbidden.
#[derive(Debug, Clone)]
pub struct PreparedPlan {
    pub(super) source_admission_id: Uuid7,
    pub(super) accepted_revision_id: Uuid7,
    pub(super) digest: String,
    pub(super) items: Vec<super::PreparedItem>,
    pub(super) authorization: Option<afd_wire::task::TaskBindingsInput>,
}

/// One atomic plan's native item identities in their original order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBindings {
    /// Stable task and item associations for every accepted action.
    pub items: Vec<TaskBinding>,
    /// Native objective attached to this lease after the complete binding.
    pub current_task: Option<TaskContext<'static>>,
    /// Immutable runtime ceiling read under the current lease ownership locks.
    pub execution_deadline: jiff::Timestamp,
    /// True when the complete original interpretation was recovered.
    pub replayed: bool,
}

/// Whether admission allocated this owner or recovered its existing identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBinding {
    /// Stable identity of the interpreted source item.
    pub item_id: Uuid7,
    /// Stable objective owner across all its execution leases.
    pub task_id: Option<Uuid7>,
    /// Current generation, without granting execution of another task.
    pub task_generation: Option<u64>,
    /// Native route, without inventing a task for conversational input.
    pub action: afd_wire::task::TaskInputAction,
    /// Current execution may apply the accepted item at its next safe boundary.
    pub applies_here: bool,
    /// Per-task accepted input order, absent for a new goal or conversation.
    pub input_sequence: Option<u64>,
}

/// A native workspace read, including the version used by public `ETags`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// The current native wait's saved exact-head observations.
    pub dependency: Option<afd_wire::task::TaskDependency>,
    pub(super) dependency_digest: Option<String>,
    /// Latest native publication attempt, including an unconfirmed outcome.
    pub publication: Option<afd_wire::task::TaskEffect>,
    pub(super) publication_digest: Option<String>,
    /// Native state and cumulative usage.
    pub context: TaskContext<'static>,
    /// User-facing compact name.
    pub title: String,
    /// Version of the entire native resource, including accounted progress.
    pub resource_version: u64,
    /// Human-readable immutable setup revision number.
    pub accepted_revision_number: u64,
}

impl TaskSnapshot {
    /// Strong resource tag shared by reads and conditional controls.
    #[must_use]
    pub fn etag(&self) -> String {
        afd_core::etag::compute(&[
            Some(self.context.task_id.as_bytes()),
            Some(&self.resource_version.to_be_bytes()),
            self.dependency_digest.as_deref().map(str::as_bytes),
            self.publication_digest.as_deref().map(str::as_bytes),
        ])
    }
}

/// One bounded page of unfinished work, newest creation time and identity first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPage {
    /// Compact rows without objective text or evidence.
    pub items: Vec<TaskSummary>,
    /// Whether another row exists beyond this page.
    pub more: bool,
}

/// Native keyset boundary for a page ordered by creation time and identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskAfter {
    /// Creation instant of the last returned row.
    pub created_at: jiff::Timestamp,
    /// Identity breaking ties at the same instant.
    pub id: Uuid7,
}

/// Confirmed native progress suitable for the fleet's current-work panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    /// Stable objective identity.
    pub id: String,
    /// User-facing compact name.
    pub title: String,
    /// Current durable lifecycle state.
    pub state: TaskState,
    /// Version of this native resource.
    pub resource_version: u64,
    /// Generation required by subsequent task operations.
    pub generation: u64,
    /// Setup revision accepted by this objective.
    pub accepted_revision_id: String,
    /// Admission instant used by the keyset boundary.
    pub created_at: jiff::Timestamp,
    /// Fixed cumulative wall deadline.
    pub expires_at: jiff::Timestamp,
}

/// Public lifecycle controls; completion is available only through native proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskControl {
    /// Stop admitting new paid work without discarding accepted progress.
    Pause,
    /// Make paused/blocked work eligible without resetting its limits.
    Resume,
    /// Fence new effects and retain reconciliation of already-issued work.
    Cancel,
}

impl TaskControl {
    pub(super) const fn requested(self) -> TaskState {
        match self {
            Self::Pause => TaskState::Paused,
            Self::Resume => TaskState::Ready,
            Self::Cancel => TaskState::Cancelled,
        }
    }
}

/// An explicit human adoption, separate from lifecycle state and execution authority.
#[derive(Debug, Clone)]
pub struct TaskAdoption {
    /// Reviewed immutable fleet setup containing the new completion wording.
    pub goal_revision_id: Uuid7,
    /// Stable operation identity for idempotent adoption.
    pub operation_id: Uuid7,
    /// Generation the person read before choosing the new goal revision.
    pub expected_generation: u64,
}
