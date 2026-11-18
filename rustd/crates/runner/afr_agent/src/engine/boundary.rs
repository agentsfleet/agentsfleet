//! Capture has a committed receipt or a typed skip; absence never means success.

use super::TurnSnapshot;
use crate::{Result, TaskRecovery};
use af_wire::lease::{TurnResume, TurnScope, TurnWorkspace};
use garde::Validate as _;

/// Native tasks and ordinary events use different durable publication owners.
#[derive(Debug)]
pub enum RecoveryState {
    /// Existing task checkpoint resources and native input/effect cursors.
    Task(TaskRecovery),
    /// Event-owned private state and exact incorporated fold membership.
    Turn(TurnSnapshot),
}

/// Owned private state sent to the supervisor's capture owner.
#[derive(Debug)]
pub struct RecoverySnapshot {
    /// Current attempt, retained in the native request's authority proof.
    pub lease_id: String,
    /// Stable event whose root was captured.
    pub event_id: String,
    /// Current attempt's database fence.
    pub fencing_token: u64,
    /// Exact private state for the selected durable owner.
    pub state: RecoveryState,
}

/// The committed identity retains the publication family's compatibility facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommittedRecovery {
    /// Native task publication already verified the matching file manifest.
    Task {
        /// Committed checkpoint identity.
        checkpoint_id: String,
        /// Monotonic checkpoint ordering.
        generation: u64,
        /// Digest of the exact captured private root.
        history_digest: String,
    },
    /// Complete ordinary checkpoint descriptor after atomic head publication.
    Turn(TurnResume),
}

/// The capture owner returns this only after commit and successful cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReceipt {
    /// Attempt that authorized publication.
    pub lease_id: String,
    /// Stable event owner.
    pub event_id: String,
    /// Fence proved by publication.
    pub fencing_token: u64,
    /// Committed identity and compatibility facts for the selected family.
    pub checkpoint: CommittedRecovery,
}

/// A safe boundary was unavailable; keep the previous generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverySkip {
    /// A child is running, queued or unwinding.
    BusyChildren,
    /// A process session is open or reserved.
    BusySessions,
    /// An executor file job outlived its caller or has not finished.
    BusyExecutor,
    /// A frozen tenant process remains.
    BusyProcesses,
    /// The matching workspace is not ready.
    WorkspaceBuilding,
    /// No capture owner or storage is available.
    Unavailable,
    /// Private state or files exceed a declared bound.
    Limit,
}

/// Only a committed receipt may advance the root's recovery generation.
#[derive(Debug)]
pub enum RecoverySave {
    /// Native publication and cleanup both completed.
    Committed(Box<RecoveryReceipt>),
    /// No new generation was acknowledged.
    Skipped(RecoverySkip),
}

/// Owned by the supervisor outside the lifetime of the agent future.
#[async_trait::async_trait]
pub trait RecoveryBoundarySink: Send + Sync + std::fmt::Debug {
    /// Current workspace custody; a building file run cannot claim API-only state.
    ///
    /// # Errors
    /// A typed skip explains why no matching workspace is ready.
    fn workspace(&self) -> std::result::Result<TurnWorkspace, RecoverySkip>;

    /// Save at a proved boundary and finish thaw or teardown before replying.
    ///
    /// # Errors
    /// Cleanup could not permit continuation; ordinary unavailable captures return a skip.
    async fn save(&self, snapshot: RecoverySnapshot) -> Result<RecoverySave>;
}

/// Correlation witness retained while the capture owner owns the private bytes.
pub(crate) struct RecoveryWitness {
    lease_id: String,
    event_id: String,
    fence: u64,
    generation: u64,
    digest: String,
    previous_id: Option<String>,
    scope: Option<TurnScope>,
}

impl RecoverySnapshot {
    pub(crate) fn witness(&self) -> RecoveryWitness {
        let (generation, digest, previous_id, scope) = match &self.state {
            RecoveryState::Task(task) => (
                task.task.checkpoint_generation.unwrap_or(0),
                af_core::etag::sha256_hex(&task.history),
                task.task.checkpoint_id.as_ref().map(ToString::to_string),
                None,
            ),
            RecoveryState::Turn(turn) => (
                turn.previous
                    .as_ref()
                    .map_or(0, |previous| previous.generation),
                af_core::etag::sha256_hex(&turn.history),
                turn.previous
                    .as_ref()
                    .map(|previous| previous.checkpoint_id.0.clone()),
                Some(turn.scope.clone()),
            ),
        };
        RecoveryWitness {
            lease_id: self.lease_id.clone(),
            event_id: self.event_id.clone(),
            fence: self.fencing_token,
            generation,
            digest,
            previous_id,
            scope,
        }
    }
}

impl RecoveryReceipt {
    pub(crate) fn matches(&self, expected: &RecoveryWitness) -> bool {
        self.lease_id == expected.lease_id
            && self.event_id == expected.event_id
            && self.fencing_token == expected.fence
            && self.checkpoint.generation() > expected.generation
            && af_core::id::Uuid7::parse(self.checkpoint.id()).is_ok()
            && expected.previous_id.as_deref() != Some(self.checkpoint.id())
            && self.checkpoint.digest() == expected.digest
            && match (&self.checkpoint, &expected.scope) {
                (CommittedRecovery::Task { .. }, None) => true,
                (CommittedRecovery::Turn(descriptor), Some(scope)) => {
                    descriptor.scope == *scope && descriptor.validate().is_ok()
                }
                _ => false,
            }
    }
}

impl CommittedRecovery {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Task { checkpoint_id, .. } => checkpoint_id,
            Self::Turn(descriptor) => &descriptor.checkpoint_id.0,
        }
    }

    pub(crate) const fn generation(&self) -> u64 {
        match self {
            Self::Task { generation, .. } => *generation,
            Self::Turn(descriptor) => descriptor.generation,
        }
    }

    fn digest(&self) -> &str {
        match self {
            Self::Task { history_digest, .. } => history_digest,
            Self::Turn(descriptor) => &descriptor.history_digest,
        }
    }
}
