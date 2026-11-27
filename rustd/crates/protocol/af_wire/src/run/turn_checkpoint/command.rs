//! Every turn request carries a stable semantic operation and the live attempt fence.

use super::{
    CheckpointOperationId, FoldRef, TURN_CHECKPOINT_FORMAT, TURN_CHECKPOINT_PAGE_BYTES,
    TurnCheckpointKind, TurnResume, TurnWorkspace,
};
use crate::durable_task::{
    CheckpointPage, CheckpointPlaintext, CheckpointTransferPurpose, CheckpointUpload,
};
use crate::protocol::MESSAGE_BYTES_MAX;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Private root bytes never appear in debug output.
#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnPrivateRoot {
    /// Versioned UTF-8 JSON; complete-message size is checked independently.
    #[garde(length(bytes, min = 1, max = MESSAGE_BYTES_MAX))]
    pub history: String,
}

impl std::fmt::Debug for TurnPrivateRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnPrivateRoot")
            .field("bytes", &self.history.len())
            .finish_non_exhaustive()
    }
}

/// Scope is derived from the run; the caller supplies only capture compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointBegin {
    /// Exact last acknowledged head; absence means no previous checkpoint.
    #[garde(dive)]
    pub expected_head: Option<CheckpointOperationId>,
    /// Independent private root format.
    #[garde(range(min = TURN_CHECKPOINT_FORMAT, max = TURN_CHECKPOINT_FORMAT))]
    pub format_version: u32,
    /// A file run can never claim the API-only variant while building.
    #[garde(dive)]
    pub workspace: TurnWorkspace,
    /// Bind operation replay to the exact captured root before any upload.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub history_digest: String,
    /// Empty only for an API-only capture. Bounded by the Postgres `INTEGER`
    /// that numbers each page, `runner.checkpoint_pages.page_number`.
    #[garde(range(max = i32::MAX.unsigned_abs()))]
    pub page_count: u32,
    /// Full-capture ceiling, including authentication tags. Bounded by its
    /// Postgres `BIGINT` column.
    #[garde(range(max = i64::MAX.unsigned_abs()))]
    pub upload_budget_bytes: u64,
    /// Expanded workspace bytes, independent of deduplication. Bounded by its
    /// Postgres `BIGINT` column.
    #[garde(range(max = i64::MAX.unsigned_abs()))]
    pub logical_bytes: u64,
    /// Complete manifest entry count. Bounded by its Postgres `INTEGER` column.
    #[garde(range(max = i32::MAX.unsigned_abs()))]
    pub entry_count: u32,
}

/// Exact generation and pin; these confer no authority without the current run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointRef {
    /// Native plan identity.
    #[garde(dive)]
    pub checkpoint_id: CheckpointOperationId,
    /// Current reader or writer custody.
    #[garde(dive)]
    pub pin_id: CheckpointOperationId,
}

/// One bounded native operation; expected response kind is derived from its action.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointCommand {
    /// Current run and thread must both prove this fence.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub fencing_token: u64,
    /// Reconnect retries retain this identity with new transport request IDs.
    #[garde(dive)]
    pub operation_id: CheckpointOperationId,
    /// No operation may select another event or arbitrary bucket path.
    #[garde(dive)]
    pub action: TurnCheckpointAction,
}

/// Keys and private root state stay within the authenticated runner session.
///
/// Each variant carries its own request, so a caller holding one knows the
/// one response variant that answers it ([`super::TurnCheckpointRequest`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnCheckpointAction {
    /// Reserve one pending generation.
    Begin(TurnCheckpointBegin),
    /// Register one quota-bounded batch.
    Objects(TurnCheckpointObjects),
    /// Freeze exact ciphertext representations and sign their create-only PUTs.
    Uploads(TurnCheckpointUploads),
    /// Authorize a restore's reads of referenced objects within the pin deadline.
    Authorize(TurnCheckpointAuthorize),
    /// Publish one bounded immutable page.
    PageWrite(TurnCheckpointPageWrite),
    /// Read the exact page named by the plan.
    PageRead(TurnCheckpointPageRead),
    /// Renew without extending the original operation deadline.
    PinRenew(TurnCheckpointPinRenew),
    /// End exact reader or writer custody.
    PinRelease(TurnCheckpointPinRelease),
    /// Seal root state and publish the complete manifest/head together.
    Commit(TurnCheckpointCommit),
    /// An assignment is a descriptor, never inline private history.
    Read(TurnCheckpointRead),
    /// Adoption validates the saved membership; it never trusts a cursor alone.
    Adopt(TurnCheckpointAdopt),
    /// Failed captures never become the event head.
    Abandon(TurnCheckpointAbandon),
}

/// Register one quota-bounded batch.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointObjects {
    /// Exact pending generation and writer custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Immutable plaintext descriptors, before encryption or upload.
    #[garde(length(min = 1), dive)]
    pub objects: Vec<CheckpointPlaintext>,
}

/// Freeze exact ciphertext representations and sign their create-only PUTs.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointUploads {
    /// Exact pending generation and writer custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Ciphertext digests and sizes bound to registered objects.
    #[garde(length(min = 1), dive)]
    pub objects: Vec<CheckpointUpload>,
}

/// Authorize only referenced objects within the current pin deadline.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointAuthorize {
    /// Generation and custody authorizing this transfer.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Writer upload or reader restore, checked against the pin.
    #[garde(skip)]
    pub purpose: CheckpointTransferPurpose,
    /// Registered immutable objects within this generation.
    #[garde(length(min = 1), dive)]
    pub object_ids: Vec<CheckpointOperationId>,
}

/// Publish one bounded immutable page.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointPageWrite {
    /// Exact pending generation and writer custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Preallocated page identity from the plan.
    #[garde(dive)]
    pub page_id: CheckpointOperationId,
    /// Digest of the serialized page contents.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub digest: String,
    /// Complete page whose encoded size is also bounded.
    #[garde(dive, custom(page_bound))]
    pub contents: CheckpointPage,
}

/// Read the exact page named by the plan.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointPageRead {
    /// Generation and live reader custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Exact immutable page identity.
    #[garde(dive)]
    pub page_id: CheckpointOperationId,
}

/// Renew without extending the original operation deadline.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointPinRenew {
    /// Exact custody being renewed.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Compare-and-swap version of the last acknowledged pin.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub resource_version: u64,
}

/// End exact reader or writer custody.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointPinRelease {
    /// Exact custody being released.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
}

/// Seal root state and publish the complete manifest/head together.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointCommit {
    /// Exact pending generation and writer custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
    /// Private state bound to the begin request's history digest.
    #[garde(dive)]
    pub root: TurnPrivateRoot,
    /// Digest of the complete ordered manifest.
    #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
    pub manifest_root: String,
    /// Exact input membership incorporated by the saved root.
    #[garde(dive)]
    pub folds: Vec<FoldRef>,
    /// Objects of this plan whose bound PUT the runner saw accepted. The
    /// commit records `verified_at` for these. It refuses any object the
    /// generation names that has none.
    #[garde(dive)]
    pub uploaded: Vec<CheckpointOperationId>,
}

/// An assignment is a descriptor, never inline private history.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointRead {
    /// Exact assignment metadata; the server never silently selects a newer head.
    #[garde(dive)]
    pub descriptor: Box<TurnResume>,
}

/// Adoption validates the saved membership; it never trusts a cursor alone.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointAdopt {
    /// Generation and reader custody used to validate the restored root.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
}

/// Failed captures never become the event head.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnCheckpointAbandon {
    /// Exact pending generation and writer custody.
    #[garde(dive)]
    pub checkpoint: TurnCheckpointRef,
}

/// The tag is flattened into the action's own fields, so a refusal names
/// `action.contents` as the wire does; the derive would add the newtype's `[0]`.
impl Validate for TurnCheckpointAction {
    type Context = ();

    fn validate_into(
        &self,
        ctx: &Self::Context,
        parent: &mut dyn FnMut() -> garde::Path,
        report: &mut garde::Report,
    ) {
        match self {
            Self::Begin(action) => action.validate_into(ctx, parent, report),
            Self::Objects(action) => action.validate_into(ctx, parent, report),
            Self::Uploads(action) => action.validate_into(ctx, parent, report),
            Self::Authorize(action) => action.validate_into(ctx, parent, report),
            Self::PageWrite(action) => action.validate_into(ctx, parent, report),
            Self::PageRead(action) => action.validate_into(ctx, parent, report),
            Self::PinRenew(action) => action.validate_into(ctx, parent, report),
            Self::PinRelease(action) => action.validate_into(ctx, parent, report),
            Self::Commit(action) => action.validate_into(ctx, parent, report),
            Self::Read(action) => action.validate_into(ctx, parent, report),
            Self::Adopt(action) => action.validate_into(ctx, parent, report),
            Self::Abandon(action) => action.validate_into(ctx, parent, report),
        }
    }
}

impl TurnCheckpointAction {
    /// Correlation never infers a successful action from an optional field.
    #[must_use]
    pub const fn kind(&self) -> TurnCheckpointKind {
        match self {
            Self::Begin(_) => TurnCheckpointKind::Begin,
            Self::Objects(_) => TurnCheckpointKind::Objects,
            Self::Uploads(_) => TurnCheckpointKind::Uploads,
            Self::Authorize(_) => TurnCheckpointKind::Authorize,
            Self::PageWrite(_) => TurnCheckpointKind::PageWrite,
            Self::PageRead(_) => TurnCheckpointKind::PageRead,
            Self::PinRenew(_) => TurnCheckpointKind::PinRenew,
            Self::PinRelease(_) => TurnCheckpointKind::PinRelease,
            Self::Commit(_) => TurnCheckpointKind::Commit,
            Self::Read(_) => TurnCheckpointKind::Read,
            Self::Adopt(_) => TurnCheckpointKind::Adopt,
            Self::Abandon(_) => TurnCheckpointKind::Abandon,
        }
    }
}

fn page_bound<C: ?Sized>(page: &CheckpointPage, _context: &C) -> garde::Result {
    if af_validate::serialized_within(page, TURN_CHECKPOINT_PAGE_BYTES) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "turn manifest page exceeds its byte limit",
        ))
    }
}
