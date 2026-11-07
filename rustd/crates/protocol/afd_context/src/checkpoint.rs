//! A checkpoint binds model-authored notes to the exact source it replaced.

use garde::Validate as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{Group, Result, Summary, error::ErrorKind};

const FORMAT_VERSION: u32 = 1;
const MODEL_IDENTITY_BYTES: usize = 1024;

/// Host-owned identity; model and trusted pinned state changes invalidate restore.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// A private draft or runtime run/session scope, never an account-wide scope.
    #[garde(
        length(bytes, min = 1, max = 256),
        custom(afd_validate::rules::nul_free)
    )]
    pub scope: String,
    /// Selected provider, model, inference settings and estimator version digest.
    #[garde(
        length(bytes, min = 1, max = MODEL_IDENTITY_BYTES),
        custom(afd_validate::rules::nul_free)
    )]
    pub model: String,
    /// Authoritative current instructions/state identity.
    #[garde(
        length(bytes, min = 1, max = 128),
        custom(afd_validate::rules::nul_free)
    )]
    pub pinned: String,
}

/// Versioned conversation working state, separate from immutable fleet revisions.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    #[garde(range(min = FORMAT_VERSION, max = FORMAT_VERSION))]
    format_version: u32,
    /// The identity whose source and pinned state were checked.
    #[garde(dive)]
    pub identity: Identity,
    /// Monotonic publication generation.
    #[garde(range(min = 1, max = i64::MAX as u64))]
    pub generation: u64,
    /// Inclusive source watermark; current requests are never part of this prefix.
    #[garde(range(min = 1))]
    pub covered_through: u64,
    /// Chained digest of every complete source group through the watermark.
    #[garde(length(bytes, min = 64, max = 64), custom(afd_validate::rules::charset(|value| value.is_ascii_hexdigit())))]
    pub source_digest: String,
    /// Untrusted notes, never system authority.
    #[garde(dive)]
    pub summary: Summary,
}

impl Checkpoint {
    /// Validate a complete document before restore or durable publication.
    ///
    /// # Errors
    /// Unsupported format, invalid identity, watermark, digest or summary.
    pub fn validate_document(&self) -> Result<()> {
        self.validate().map_err(|_untrusted| ErrorKind::Stale)?;
        self.summary.validate_document()?;
        Ok(())
    }

    /// A prepared publication after the model response has been validated.
    pub(crate) fn new(task: &crate::Compaction, summary: Summary) -> Result<Self> {
        summary.validate_document()?;
        if task.groups.is_empty() {
            return Err(ErrorKind::Stale.into());
        }
        let generation = task
            .expected_generation
            .checked_add(1)
            .ok_or(ErrorKind::Stale)?;
        Ok(Self {
            format_version: FORMAT_VERSION,
            identity: task.identity.clone(),
            generation,
            covered_through: task.covered_through(),
            source_digest: digest_groups(&task.prior_digest, &task.groups),
            summary,
        })
    }

    /// Whether this format belongs to the current scope and selected model/state.
    #[must_use]
    pub fn belongs_to(&self, identity: &Identity) -> bool {
        self.format_version == FORMAT_VERSION && self.generation > 0 && &self.identity == identity
    }

    /// Reject a checkpoint whose covered source has changed.
    ///
    /// # Errors
    /// The source watermark or digest no longer matches.
    pub fn verify_source(&self, sequence: u64, digest: &str) -> Result<()> {
        if sequence != self.covered_through || digest != self.source_digest {
            return Err(ErrorKind::Stale.into());
        }
        Ok(())
    }
}

/// Stable chaining avoids retaining old source text merely to validate a later page.
#[must_use]
pub fn digest_groups(previous: &str, groups: &[Group]) -> String {
    groups.iter().fold(previous.to_owned(), |digest, group| {
        let mut hash = Sha256::new();
        hash.update(digest.as_bytes());
        hash.update(group.sequence.to_be_bytes());
        hash.update((group.text.len() as u64).to_be_bytes());
        hash.update(group.text.as_bytes());
        hex::encode(hash.finalize())
    })
}
