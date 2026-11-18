//! Pushed work and its two authority-separated placement payloads.

mod kinds;
mod payload;
mod turn_checkpoint;

use serde::{Deserialize, Serialize};

pub use kinds::{AcceptedKinds, LeaseKind};
pub use payload::{
    ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX,
    LeasePayload, TURN_TEXT_BYTES_MAX, Turn,
};
pub use turn_checkpoint::{
    FoldRef, TURN_CHECKPOINT_BYTES, TURN_CHECKPOINT_FORMAT, TURN_CHECKPOINT_PAGE_BYTES,
    TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand, TurnCheckpointKind,
    TurnCheckpointPlan, TurnCheckpointRead, TurnCheckpointRef, TurnCheckpointResponse, TurnId,
    TurnMessageSize, TurnPrivateRoot, TurnResume, TurnScope, TurnWorkspace,
};

/// Placement identifies the execution authority carried by a pushed offer.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedLease<'a, C = serde_json::Value> {
    /// Installed fleet work, with its runtime policy.
    Fleet(#[serde(borrow)] Box<LeasePayload<'a>>),
    /// Private setup work, with access to the daemon's scoped model endpoint.
    Authoring(Box<crate::authoring::Lease<C>>),
}
