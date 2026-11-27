//! Capture has a committed receipt or a typed skip; absence never means success.

use super::TurnSnapshot;
use crate::{DurableTaskRecovery, Result};
use af_wire::run::{TurnResume, TurnScope, TurnWorkspace};
use garde::Validate as _;
use std::sync::Arc;

/// A runner-wide file-operation reservation stays charged while its owner runs.
pub type FileRecoveryPermit = Arc<tokio::sync::OwnedSemaphorePermit>;

/// Native tasks and ordinary events use different durable publication owners.
#[derive(Debug)]
pub enum RecoveryState {
    /// Existing task checkpoint resources and native input/effect cursors.
    DurableTask(DurableTaskRecovery),
    /// Event-owned private state and exact incorporated fold membership.
    Turn(TurnSnapshot),
}

/// Owned private state sent to the supervisor's capture owner.
#[derive(Debug)]
pub struct RecoverySnapshot {
    /// Current attempt, retained in the native request's authority proof.
    pub run_id: String,
    /// Stable event whose root was captured.
    pub turn_id: String,
    /// Current attempt's database fence.
    pub fencing_token: u64,
    /// Exact private state for the selected durable owner.
    pub state: RecoveryState,
    /// Admission transferred to the supervisor along with the private state.
    pub file_permit: Option<FileRecoveryPermit>,
}

/// The committed identity retains the publication family's compatibility facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommittedRecovery {
    /// Native task publication already verified the matching file manifest.
    DurableTask {
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
    pub run_id: String,
    /// Stable event owner.
    pub turn_id: String,
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
    /// A guest agent file job outlived its caller or has not finished.
    BusyAgent,
    /// A frozen tenant process remains.
    BusyProcesses,
    /// Other checkpoint or restore work is using the runner's file capacity.
    BusyStorage,
    /// The run's previous save is still publishing: one publication runs
    /// per owner, and this boundary never paused.
    BusyPublication,
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
    /// The run resumed at the pin; the save publishes on, and its receipt
    /// follows.
    Publishing(Publication),
    /// No new generation was acknowledged.
    Skipped(RecoverySkip),
}

/// What a save that resumed its run sends once its publication ends: a
/// committed receipt, a skip, or the failure that ended it.
pub type Published = Result<RecoverySave>;

/// A save publishing after its run resumed. The run reads the receipt at its
/// next boundary or its end, never sooner, so no model call waits on it.
#[derive(Debug)]
pub struct Publication {
    receipt: tokio::sync::oneshot::Receiver<Published>,
}

impl Publication {
    /// A publication, and the sender its owner delivers the receipt through.
    #[must_use]
    pub fn channel() -> (tokio::sync::oneshot::Sender<Published>, Self) {
        let (send, receipt) = tokio::sync::oneshot::channel();
        (send, Self { receipt })
    }

    /// The receipt once publication has ended, or `None` while it runs.
    pub(crate) fn try_settled(&mut self) -> Option<Published> {
        match self.receipt.try_recv() {
            Ok(published) => Some(published),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Err(lost())),
        }
    }

    /// The receipt, waiting for publication to end.
    ///
    /// # Errors
    /// The publication failed, or its owner went before it answered.
    pub async fn settled(self) -> Published {
        self.receipt.await.unwrap_or_else(|_closed| Err(lost()))
    }
}

/// The owner went before it said how its publication ended.
fn lost() -> crate::Error {
    crate::error::recovery_invalid("the checkpoint publication ended without a receipt")
}

/// Owned by the supervisor outside the lifetime of the agent future.
#[async_trait::async_trait]
pub trait RecoveryBoundarySink: Send + Sync + std::fmt::Debug {
    /// Current workspace custody; a building file run cannot claim API-only state.
    ///
    /// # Errors
    /// A typed skip explains why no matching workspace is ready.
    fn workspace(&self) -> std::result::Result<TurnWorkspace, RecoverySkip>;

    /// Reserve before cloning private state or freezing the workspace.
    /// Durable task history also needs storage when the task uses no sandbox.
    ///
    /// # Errors
    /// A typed skip explains why the boundary cannot reserve file work.
    fn try_reserve(
        &self,
        task: bool,
    ) -> std::result::Result<Option<FileRecoveryPermit>, RecoverySkip>;

    /// Save at a proved boundary and finish thaw or teardown before replying.
    ///
    /// # Errors
    /// Cleanup could not permit continuation; ordinary unavailable captures return a skip.
    async fn save(&self, snapshot: RecoverySnapshot) -> Result<RecoverySave>;
}

/// Correlation witness retained while the capture owner owns the private bytes.
pub(crate) struct RecoveryWitness {
    run_id: String,
    turn_id: String,
    fencing_token: u64,
    generation: u64,
    digest: String,
    previous_id: Option<String>,
    scope: Option<TurnScope>,
}

impl RecoverySnapshot {
    pub(crate) fn witness(&self) -> RecoveryWitness {
        let (generation, digest, previous_id, scope) = match &self.state {
            RecoveryState::DurableTask(task) => (
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
            run_id: self.run_id.clone(),
            turn_id: self.turn_id.clone(),
            fencing_token: self.fencing_token,
            generation,
            digest,
            previous_id,
            scope,
        }
    }
}

impl RecoveryReceipt {
    pub(crate) fn matches(&self, expected: &RecoveryWitness) -> bool {
        self.run_id == expected.run_id
            && self.turn_id == expected.turn_id
            && self.fencing_token == expected.fencing_token
            && self.checkpoint.generation() > expected.generation
            && af_core::id::Uuid7::parse(self.checkpoint.id()).is_ok()
            && expected.previous_id.as_deref() != Some(self.checkpoint.id())
            && self.checkpoint.digest() == expected.digest
            && match (&self.checkpoint, &expected.scope) {
                (CommittedRecovery::DurableTask { .. }, None) => true,
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
            Self::DurableTask { checkpoint_id, .. } => checkpoint_id,
            Self::Turn(descriptor) => &descriptor.checkpoint_id.0,
        }
    }

    pub(crate) const fn generation(&self) -> u64 {
        match self {
            Self::DurableTask { generation, .. } => *generation,
            Self::Turn(descriptor) => descriptor.generation,
        }
    }

    fn digest(&self) -> &str {
        match self {
            Self::DurableTask { history_digest, .. } => history_digest,
            Self::Turn(descriptor) => &descriptor.history_digest,
        }
    }
}
