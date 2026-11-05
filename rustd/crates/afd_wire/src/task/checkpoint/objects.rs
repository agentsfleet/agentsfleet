//! Bounded plaintext registration and immutable transfer descriptors.

use std::collections::BTreeMap;
use garde::Validate;
use serde::{Deserialize, Serialize};
use crate::task::DecisionTaskVersion;
use super::CheckpointTransferPurpose;

/// Trusted capture's plaintext descriptor; no storage key is caller-selected.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPlaintext {
    /// Private plaintext SHA-256, verified again after decryption.
    #[garde(custom(super::digest))] pub sha256: String,
    /// Bounded plaintext chunk length.
    #[garde(range(min = 1, max = 8_388_608))] pub size: u64,
    /// Declared file, Git or history bytes.
    #[garde(length(bytes, min = 1, max = 32), pattern(r"^[a-z_]+$"))] pub kind: String,
}

/// Register a batch under one quota and ownership transaction.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointObjectsInput {
    /// Existing lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Current writer pin.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub pin_id: String,
    /// At most one descriptor page.
    #[garde(length(min = 1, max = 256), dive)] pub objects: Vec<CheckpointPlaintext>,
}

/// One frozen ciphertext representation, registered before its first URL.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointUpload {
    /// Native registration identity.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub object_id: String,
    /// Exact ciphertext and authentication tag SHA-256.
    #[garde(custom(super::digest))] pub ciphertext_sha256: String,
    /// Plaintext length plus the fixed authentication tag.
    #[garde(range(min = 17, max = 8_388_624))] pub ciphertext_length: u64,
}

/// Freeze an entire descriptor page atomically.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointUploadsInput {
    /// Existing lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Current writer pin.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"))] pub pin_id: String,
    /// At most one immutable descriptor page.
    #[garde(length(min = 1, max = 256), dive)] pub objects: Vec<CheckpointUpload>,
}

/// Acquire a restore pin, or refresh exact registered transfer material.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointAuthorizationInput {
    /// Existing lease fence.
    #[garde(skip)] pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)] pub task: DecisionTaskVersion,
    /// Read or upload authority, each checked natively.
    #[garde(skip)] pub purpose: CheckpointTransferPurpose,
    /// Omitted only to bootstrap a restore pin and ordered pages.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))] pub pin_id: Option<String>,
    /// Native object identities, never arbitrary bucket paths.
    #[serde(default)]
    #[garde(length(max = 256), inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")))] pub object_ids: Vec<String>,
}

/// Native registration disposition.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointObjectStatus { Missing, Reused, Busy }

/// Exact scoped transfer material, redacted from debug output.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckpointTransfer {
    /// Short-lived supervisor-only URL.
    pub url: String,
    /// Exact signed headers; create-only uploads cannot omit them.
    pub headers: BTreeMap<String, String>,
    /// Deadline shared with native pin custody.
    #[serde(with = "crate::instant")] pub expires_at: jiff::Timestamp,
}

impl std::fmt::Debug for CheckpointTransfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("CheckpointTransfer(redacted)") }
}

/// Registration projection; temporary keys remain outside model/sandbox state.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
pub struct CheckpointObject {
    /// Native random object identity.
    pub object_id: String,
    /// Canonical scope for authenticated encryption.
    pub repository: String,
    /// Plaintext descriptor.
    pub plaintext: CheckpointPlaintext,
    /// Current ownership disposition.
    pub status: CheckpointObjectStatus,
    /// Exact fixed nonce, base64 encoded.
    pub nonce: Option<String>,
    /// Temporary data key, returned only to this native owner/reader.
    pub data_key: Option<String>,
    /// Frozen ciphertext digest when already known.
    pub ciphertext_sha256: Option<String>,
    /// Frozen ciphertext length when already known.
    pub ciphertext_length: Option<u64>,
    /// Exact scoped URL when requested.
    pub transfer: Option<CheckpointTransfer>,
}

impl std::fmt::Debug for CheckpointObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointObject").field("object_id", &self.object_id).field("status", &self.status).finish_non_exhaustive()
    }
}

/// Bounded current object page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointObjectsPage {
    /// Complete objects for this request or bounded read page.
    pub items: Vec<CheckpointObject>,
    /// Next native identity when more objects remain.
    pub next_cursor: Option<String>,
}

/// Restore bootstrap or bounded refresh, always including the native pin.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointAuthorizations {
    /// Bounded complete recovery descriptor.
    pub plan: super::CheckpointPlan,
    /// Requested scoped transfers; empty at restore bootstrap.
    pub objects: Vec<CheckpointObject>,
}
