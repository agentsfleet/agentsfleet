//! Native task values, constructed after wire validation and target preflight.

use std::ops::Range;

use afd_core::id::Uuid7;
use afd_wire::task::{TaskContext, TaskLimits, TaskState};

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
    pub(super) items: Vec<PreparedTask>,
}

/// One atomic plan's native item identities in their original order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBindings {
    /// Stable task and item associations for every accepted action.
    pub items: Vec<TaskBinding>,
    /// True when the complete original interpretation was recovered.
    pub replayed: bool,
}

/// Whether admission allocated this owner or recovered its existing identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBinding {
    /// Stable identity of the interpreted source item.
    pub item_id: Uuid7,
    /// Stable objective owner across all its execution leases.
    pub task_id: Uuid7,
}

/// A native workspace read, including the version used by public `ETags`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// Native state and cumulative usage.
    pub context: TaskContext<'static>,
    /// User-facing compact name.
    pub title: String,
    /// Version of the entire native resource, including accounted progress.
    pub resource_version: u64,
    /// Human-readable immutable setup revision number.
    pub accepted_revision_number: u64,
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
