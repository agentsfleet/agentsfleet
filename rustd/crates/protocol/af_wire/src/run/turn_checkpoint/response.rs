//! Acknowledgements preserve the exact generation and the requested operation kind.

use super::{
    CheckpointOperationId, FoldRef, TURN_CHECKPOINT_FORMAT, TurnCheckpointKind, TurnPrivateRoot,
    TurnResume, TurnScope,
};
use crate::task::{CheckpointObject, CheckpointPage, CheckpointPin};
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Pending plans carry no usable recovery pointer until commit completes.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointPlan {
    /// Allocated identity survives exact operation replay.
    #[garde(dive)]
    pub checkpoint_id: CheckpointOperationId,
    /// Strictly increasing within the event.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub generation: u64,
    /// Root format accepted by both daemon and runner.
    #[garde(range(min = TURN_CHECKPOINT_FORMAT, max = TURN_CHECKPOINT_FORMAT))]
    pub format_version: u32,
    /// Scope derived from the frozen run and capture mode.
    #[garde(dive)]
    pub scope: TurnScope,
    /// Publication compares this exact head again.
    #[garde(dive)]
    pub expected_head: Option<CheckpointOperationId>,
    /// Complete ordered pages, allocated before their first write.
    #[garde(length(max = 64), dive)]
    pub page_ids: Vec<CheckpointOperationId>,
    /// Writer custody for this capture.
    #[garde(dive)]
    pub pin: CheckpointPin,
    /// Binds the operation to its intended root even before commit.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub history_digest: String,
    /// A retry returns this plan rather than allocating again.
    #[garde(skip)]
    pub replayed: bool,
}

/// Files and root are read under the same exact pinned descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointRead {
    /// Must equal the offer's descriptor before any model call.
    #[garde(dive)]
    pub descriptor: TurnResume,
    /// Current replacement's reader custody.
    #[garde(dive)]
    pub pin: CheckpointPin,
    /// Ordered file pages; empty for explicit API-only state.
    #[garde(length(max = 64), dive)]
    pub page_ids: Vec<CheckpointOperationId>,
    /// Complete authenticated private root; debug output is redacted.
    #[garde(dive)]
    pub root: TurnPrivateRoot,
    /// Exact saved admissions, independently checked against root bytes.
    #[garde(length(max = 16_384), dive)]
    pub folds: Vec<FoldRef>,
}

/// Only the committed variant acknowledges a usable ordinary checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TurnCheckpointResponse {
    /// Exact allocated plan and writer pin.
    Begun(Box<TurnCheckpointPlan>),
    /// Complete scoped batch in request order.
    Objects(Vec<CheckpointObject>),
    /// Complete immutable upload representations.
    Uploads(Vec<CheckpointObject>),
    /// Exact requested object transfers under the existing pin.
    Authorized(Vec<CheckpointObject>),
    /// Exact canonical page accepted by the writer.
    PageWritten(CheckpointPage),
    /// Exact canonical page read by the current pin.
    PageRead(CheckpointPage),
    /// New version and bounded deadline.
    PinRenewed(CheckpointPin),
    /// Custody was released or already released.
    PinReleased,
    /// Atomic ready/head publication finished before this receipt.
    Committed(Box<TurnResume>),
    /// Exact private root and matching manifest descriptor.
    Read(Box<TurnCheckpointRead>),
    /// Exact membership now owned by the replacement run/fence.
    Adopted(Vec<FoldRef>),
    /// Pending state was abandoned without changing the head.
    Abandoned,
}

impl TurnCheckpointResponse {
    /// A response from another operation cannot satisfy a pending request.
    #[must_use]
    pub const fn kind(&self) -> TurnCheckpointKind {
        match self {
            Self::Begun(_) => TurnCheckpointKind::Begin,
            Self::Objects(_) => TurnCheckpointKind::Objects,
            Self::Uploads(_) => TurnCheckpointKind::Uploads,
            Self::Authorized(_) => TurnCheckpointKind::Authorize,
            Self::PageWritten(_) => TurnCheckpointKind::PageWrite,
            Self::PageRead(_) => TurnCheckpointKind::PageRead,
            Self::PinRenewed(_) => TurnCheckpointKind::PinRenew,
            Self::PinReleased => TurnCheckpointKind::PinRelease,
            Self::Committed(_) => TurnCheckpointKind::Commit,
            Self::Read(_) => TurnCheckpointKind::Read,
            Self::Adopted(_) => TurnCheckpointKind::Adopt,
            Self::Abandoned => TurnCheckpointKind::Abandon,
        }
    }
}
