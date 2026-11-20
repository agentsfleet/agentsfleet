//! Pushed work and its two authority-separated placement payloads.

mod kinds;
mod payload;
mod setup;
mod turn_checkpoint;

use serde::{Deserialize, Serialize};

pub use kinds::{AcceptedKinds, RunKind};
pub use payload::{
    ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX,
    RunPayload, TURN_TEXT_BYTES_MAX, Turn,
};
pub use setup::{
    Owner, SETUP_DEFINITION_BYTES_MAX, SetupCandidate, SetupProposal, SetupReport, SetupWork,
};
pub use turn_checkpoint::{
    CheckpointOperationId, FoldRef, TURN_CHECKPOINT_BYTES, TURN_CHECKPOINT_FORMAT,
    TURN_CHECKPOINT_PAGE_BYTES, TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand,
    TurnCheckpointKind, TurnCheckpointPlan, TurnCheckpointRead, TurnCheckpointRef,
    TurnCheckpointResponse, TurnMessageSize, TurnPrivateRoot, TurnResume, TurnScope, TurnWorkspace,
};

/// Placement identifies the execution authority carried by a pushed assignment.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedRun<'a, C = serde_json::Value> {
    /// Work on the common run path, with its runtime policy: an installed
    /// fleet's event, or, on protocol v4, a setup draft's message
    /// ([`RunPayload::owner`]).
    Fleet(#[serde(borrow)] Box<RunPayload<'a>>),
    /// Private setup work, with access to the daemon's scoped model endpoint.
    Authoring(Box<crate::authoring::Run<C>>),
}
