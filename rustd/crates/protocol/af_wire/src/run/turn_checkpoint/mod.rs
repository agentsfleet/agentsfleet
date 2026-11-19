//! Ordinary recovery belongs to an event and never impersonates a native task.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Budget for the complete native command or response, including its envelope.
#[derive(Debug, Validate)]
pub struct TurnMessageSize {
    /// Serialized payload bytes, excluding only the native length prefix.
    #[garde(range(min = 1, max = TURN_CHECKPOINT_BYTES))]
    pub bytes: usize,
}

mod command;
mod response;
pub use command::{
    TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand, TurnCheckpointRef,
    TurnPrivateRoot,
};
pub use response::{TurnCheckpointPlan, TurnCheckpointRead, TurnCheckpointResponse};

/// Complete native messages, including envelope and escaping, must fit this bound.
pub const TURN_CHECKPOINT_BYTES: usize = 1024 * 1024;
/// The independently versioned private root format.
pub const TURN_CHECKPOINT_FORMAT: u32 = 1;
/// A page fits together with native correlation and authority fields.
pub const TURN_CHECKPOINT_PAGE_BYTES: usize = 512 * 1024;

/// Correlation includes the exact operation family as well as run and operation ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnCheckpointKind {
    /// Allocate a generation and writer pin.
    Begin,
    /// Register scoped immutable content.
    Objects,
    /// Freeze and verify upload representations.
    Uploads,
    /// Refresh scoped object transfer material.
    Authorize,
    /// Acknowledge immutable page bytes.
    PageWrite,
    /// Read a page through a live pin.
    PageRead,
    /// Extend custody within its original deadline.
    PinRenew,
    /// Release exact custody.
    PinRelease,
    /// Atomically publish matching files, root and folds.
    Commit,
    /// Pin and read the exact offered descriptor.
    Read,
    /// Restamp only incorporated inputs under the new fence.
    Adopt,
    /// End a pending capture without publishing it.
    Abandon,
}

/// A canonical stored identifier; domain code converts it once to its native UUID type.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Validate)]
#[serde(transparent)]
pub struct TurnId(
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub String,
);

/// An API-only run explicitly owns no workspace files.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnWorkspace {
    /// No sandbox was needed or built for this run.
    ApiOnly,
    /// A complete supported workspace captured under this toolbox.
    Files {
        /// Verified host-local toolbox; its bytes are excluded from the snapshot.
        #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
        toolbox_digest: String,
    },
}

/// Immutable compatibility witnesses from the issued run and its capture owner.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnScope {
    /// Workspace containing the event and encrypted objects.
    #[garde(dive)]
    pub workspace_id: TurnId,
    /// Fleet owning the stable event.
    #[garde(dive)]
    pub fleet_id: TurnId,
    /// Stable root event across every replacement run.
    #[garde(dive)]
    pub event_id: TurnId,
    /// Setup frozen during run issuance, independent of the later fleet head.
    #[garde(dive)]
    pub setup_revision_id: TurnId,
    /// API-only or complete files; a building workspace has no saved variant.
    #[garde(dive)]
    pub workspace: TurnWorkspace,
}

/// Exact incorporated input membership; a sequence cursor alone proves no custody.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FoldRef {
    /// Logical admission claimed and incorporated by the writer.
    #[garde(dive)]
    pub event_id: TurnId,
    /// Monotonic conversation position.
    #[garde(range(min = 1))]
    pub sequence: i64,
}

/// Offers carry only this descriptor. A fenced read supplies private state and a pin.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnResume {
    /// Immutable event-owned generation identity.
    #[garde(dive)]
    pub checkpoint_id: TurnId,
    /// Allocated monotonically, including failed and retired generations.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub generation: u64,
    /// Unsupported formats refuse before any model or tool call.
    #[garde(range(min = TURN_CHECKPOINT_FORMAT, max = TURN_CHECKPOINT_FORMAT))]
    pub format_version: u32,
    /// Scope and setup the frozen files and private root authenticate.
    #[garde(dive)]
    pub scope: TurnScope,
    /// Digest of the complete ordered manifest.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub manifest_root: String,
    /// Digest of the exact root bytes returned by the native reader.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub history_digest: String,
}
