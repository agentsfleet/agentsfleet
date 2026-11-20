//! Every turn request carries a stable semantic operation and the live attempt fence.

use super::{
    CheckpointOperationId, FoldRef, TURN_CHECKPOINT_BYTES, TURN_CHECKPOINT_FORMAT,
    TURN_CHECKPOINT_PAGE_BYTES, TurnCheckpointKind, TurnResume, TurnWorkspace,
};
use crate::task::{
    CheckpointPage, CheckpointPlaintext, CheckpointTransferPurpose, CheckpointUpload,
};
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Private root bytes never appear in debug output.
#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TurnPrivateRoot {
    /// Versioned UTF-8 JSON; complete-message size is checked independently.
    #[garde(length(bytes, min = 1, max = TURN_CHECKPOINT_BYTES))]
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
    /// Empty only for an API-only capture.
    #[garde(range(max = 64))]
    pub page_count: u32,
    /// Full-capture ceiling, including authentication tags.
    #[garde(range(max = 17_179_869_184))]
    pub upload_budget_bytes: u64,
    /// Expanded workspace bytes, independent of deduplication.
    #[garde(range(max = 25_769_803_776))]
    pub logical_bytes: u64,
    /// Complete manifest entry count.
    #[garde(range(max = 200_000))]
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
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnCheckpointAction {
    /// Reserve one pending generation.
    Begin(#[garde(dive)] TurnCheckpointBegin),
    /// Register one quota-bounded batch.
    Objects {
        /// Exact pending generation and writer custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Immutable plaintext descriptors, before encryption or upload.
        #[garde(length(min = 1, max = 256), dive)]
        objects: Vec<CheckpointPlaintext>,
    },
    /// Commit exact ciphertext representations before transfer verification.
    Uploads {
        /// Exact pending generation and writer custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Ciphertext digests and sizes bound to registered objects.
        #[garde(length(min = 1, max = 256), dive)]
        objects: Vec<CheckpointUpload>,
    },
    /// Authorize only referenced objects within the current pin deadline.
    Authorize {
        /// Generation and custody authorizing this transfer.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Writer upload or reader restore, checked against the pin.
        #[garde(skip)]
        purpose: CheckpointTransferPurpose,
        /// Registered immutable objects within this generation.
        #[garde(length(min = 1, max = 256), dive)]
        object_ids: Vec<CheckpointOperationId>,
    },
    /// Publish one bounded immutable page.
    PageWrite {
        /// Exact pending generation and writer custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Preallocated page identity from the plan.
        #[garde(dive)]
        page_id: CheckpointOperationId,
        /// Digest of the serialized page contents.
        #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
        digest: String,
        /// Complete page whose encoded size is also bounded.
        #[garde(dive, custom(page_bound))]
        contents: CheckpointPage,
    },
    /// Read the exact page named by the plan.
    PageRead {
        /// Generation and live reader custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Exact immutable page identity.
        #[garde(dive)]
        page_id: CheckpointOperationId,
    },
    /// Renew without extending the original operation deadline.
    PinRenew {
        /// Exact custody being renewed.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Compare-and-swap version of the last acknowledged pin.
        #[garde(range(min = 1, max = i64::MAX as u64))]
        resource_version: u64,
    },
    /// End exact reader or writer custody.
    PinRelease {
        /// Exact custody being released.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
    },
    /// Seal root state and publish the complete manifest/head together.
    Commit {
        /// Exact pending generation and writer custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
        /// Private state bound to the begin request's history digest.
        #[garde(dive)]
        root: TurnPrivateRoot,
        /// Digest of the complete ordered manifest.
        #[garde(length(bytes, min = 64, max = 64), pattern(r"^[0-9a-f]+$"))]
        manifest_root: String,
        /// Exact input membership incorporated by the saved root.
        #[garde(length(max = 16_384), dive)]
        folds: Vec<FoldRef>,
    },
    /// An assignment is a descriptor, never inline private history.
    Read {
        /// Exact assignment metadata; the server never silently selects a newer head.
        #[garde(dive)]
        descriptor: Box<TurnResume>,
    },
    /// Adoption validates the saved membership; it never trusts a cursor alone.
    Adopt {
        /// Generation and reader custody used to validate the restored root.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
    },
    /// Failed captures never become the event head.
    Abandon {
        /// Exact pending generation and writer custody.
        #[garde(dive)]
        checkpoint: TurnCheckpointRef,
    },
}

impl TurnCheckpointAction {
    /// Native answer budget for readback, including frame delivery.
    pub const VERIFY_ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

    /// Upload authorization in verification mode reads the stored ciphertext back.
    #[must_use]
    pub const fn verifies_uploads(&self) -> bool {
        matches!(
            self,
            Self::Authorize {
                purpose: CheckpointTransferPurpose::Upload,
                ..
            }
        )
    }

    /// Correlation never infers a successful action from an optional field.
    #[must_use]
    pub const fn kind(&self) -> TurnCheckpointKind {
        match self {
            Self::Begin(_) => TurnCheckpointKind::Begin,
            Self::Objects { .. } => TurnCheckpointKind::Objects,
            Self::Uploads { .. } => TurnCheckpointKind::Uploads,
            Self::Authorize { .. } => TurnCheckpointKind::Authorize,
            Self::PageWrite { .. } => TurnCheckpointKind::PageWrite,
            Self::PageRead { .. } => TurnCheckpointKind::PageRead,
            Self::PinRenew { .. } => TurnCheckpointKind::PinRenew,
            Self::PinRelease { .. } => TurnCheckpointKind::PinRelease,
            Self::Commit { .. } => TurnCheckpointKind::Commit,
            Self::Read { .. } => TurnCheckpointKind::Read,
            Self::Adopt { .. } => TurnCheckpointKind::Adopt,
            Self::Abandon { .. } => TurnCheckpointKind::Abandon,
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
