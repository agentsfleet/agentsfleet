//! Pushed work: a fleet's turn, and the checkpoint it resumes from.

mod owner;
mod payload;
mod turn_checkpoint;

use serde::{Deserialize, Serialize};

pub use owner::Owner;
pub use payload::{ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, EarlierCall, RunPayload, Turn};
pub use turn_checkpoint::{
    CheckpointOperationId, FoldRef, TURN_CHECKPOINT_FORMAT, TURN_CHECKPOINT_PAGE_BYTES,
    TurnCheckpointAbandon, TurnCheckpointAction, TurnCheckpointAdopt, TurnCheckpointAuthorize,
    TurnCheckpointBegin, TurnCheckpointCommand, TurnCheckpointCommit, TurnCheckpointKind,
    TurnCheckpointObjects, TurnCheckpointPageRead, TurnCheckpointPageWrite,
    TurnCheckpointPinRelease, TurnCheckpointPinRenew, TurnCheckpointPlan, TurnCheckpointRead,
    TurnCheckpointRef, TurnCheckpointRequest, TurnCheckpointResponse, TurnCheckpointSaved,
    TurnCheckpointUploads, TurnMessageSize, TurnPrivateRoot, TurnResume, TurnScope, TurnWorkspace,
};

/// A pushed assignment, tagged by the work it carries.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedRun<'a> {
    /// A fleet's turn, with its runtime policy.
    Fleet(#[serde(borrow)] Box<RunPayload<'a>>),
}
