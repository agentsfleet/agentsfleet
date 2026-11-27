//! Bounded plaintext registration and immutable transfer descriptors.

use super::CheckpointTransferPurpose;
use crate::durable_task::DecisionDurableTaskVersion;
use garde::Validate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Trusted capture's plaintext descriptor; no storage key is caller-selected.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointPlaintext {
    /// Private plaintext SHA-256, verified again after decryption.
    #[garde(custom(super::digest))]
    pub sha256: String,
    /// Plaintext chunk length in bytes.
    #[garde(range(min = 1, max = i64::MAX.unsigned_abs()))]
    pub size: u64,
    /// Declared file, Git or history bytes.
    #[garde(
        length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS),
        pattern(r"^[a-z_]+$")
    )]
    pub kind: String,
}

/// Register a batch under one quota and ownership transaction.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointObjectsInput {
    /// Existing run fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)]
    pub durable_task: DecisionDurableTaskVersion,
    /// Current writer pin.
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub pin_id: String,
    /// One descriptor page.
    #[garde(length(min = 1), dive)]
    pub objects: Vec<CheckpointPlaintext>,
}

/// One frozen ciphertext representation, registered before its first URL.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointUpload {
    /// Native registration identity.
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub object_id: String,
    /// SHA-256 of the object as stored.
    #[garde(custom(super::digest))]
    pub ciphertext_sha256: String,
    /// Stored length: the plaintext, plus the 16-byte authentication tag under
    /// `aes_256_gcm`. Bounded by its Postgres `BIGINT` column.
    #[garde(range(min = 1, max = i64::MAX.unsigned_abs()))]
    pub ciphertext_length: u64,
}

/// Freeze an entire descriptor page atomically.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointUploadsInput {
    /// Existing run fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)]
    pub durable_task: DecisionDurableTaskVersion,
    /// Current writer pin.
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub pin_id: String,
    /// One immutable descriptor page.
    #[garde(length(min = 1), dive)]
    pub objects: Vec<CheckpointUpload>,
}

/// Acquire a restore pin, or refresh exact registered transfer material.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CheckpointAuthorizationInput {
    /// Existing run fence.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Current task version.
    #[garde(dive)]
    pub durable_task: DecisionDurableTaskVersion,
    /// Read or upload authority, each checked natively.
    #[garde(skip)]
    pub purpose: CheckpointTransferPurpose,
    /// Omitted only to bootstrap a restore pin and ordered pages.
    #[garde(inner(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    ))]
    pub pin_id: Option<String>,
    /// Native object identities, never arbitrary bucket paths.
    #[serde(default)]
    #[garde(inner(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    ))]
    pub object_ids: Vec<String>,
}

/// What seals a checkpoint's objects and history root under its workspace's key.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::IntoStaticStr,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CheckpointCipher {
    /// AES-256-GCM: the ciphertext, then its 16-byte authentication tag.
    #[default]
    #[serde(rename = "aes_256_gcm")]
    #[strum(serialize = "aes_256_gcm")]
    Aes256Gcm,
    /// The bytes as they are, with no tag: for measuring a save and a restore
    /// before encryption is turned on.
    Plain,
}

/// Native registration disposition.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointObjectStatus {
    /// This owner must upload ciphertext before native verification can complete.
    Missing,
    /// Verified ciphertext already exists and needs no upload.
    Reused,
    /// Another live owner holds the unfinished upload reservation.
    Busy,
}

/// Exact scoped transfer material, redacted from debug output.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckpointTransfer {
    /// Short-lived supervisor-only URL.
    pub url: String,
    /// Exact signed headers; create-only uploads cannot omit them.
    pub headers: BTreeMap<String, String>,
    /// Deadline shared with native pin custody.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
}

impl std::fmt::Debug for CheckpointTransfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CheckpointTransfer(redacted)")
    }
}

/// Registration projection; the key remains outside model/sandbox state.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckpointObject {
    /// Native random object identity.
    pub object_id: String,
    /// Canonical scope for authenticated encryption.
    pub repository: String,
    /// Plaintext descriptor.
    pub plaintext: CheckpointPlaintext,
    /// Current ownership disposition.
    pub status: CheckpointObjectStatus,
    /// The cipher this object was registered under; a restore opens with it.
    pub cipher: CheckpointCipher,
    /// The object's registered random nonce, base64 encoded.
    pub nonce: Option<String>,
    /// The workspace's checkpoint key, returned only to this native owner/reader.
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
        f.debug_struct("CheckpointObject")
            .field("object_id", &self.object_id)
            .field("status", &self.status)
            .finish_non_exhaustive()
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

/// What rotating a workspace's checkpoint key retired. The old key sealed all
/// of it, so none of it can be restored.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointKeyRotation {
    /// Checkpoint generations deleted, unfinished saves among them.
    pub retired_generations: u64,
    /// Stored objects whose deletion from the bucket is now owed.
    pub retired_objects: u64,
}
