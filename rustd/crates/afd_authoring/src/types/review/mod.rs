//! Immutable requirements and current readiness have distinct meanings and types.

mod readiness;
mod requirements;

use afd_fleet_runtime::Definition;
use serde::Serialize;

pub use readiness::{Action, ActionKind, Check, CheckKind, CheckState};
pub use requirements::Requirements;

/// Exact setup and selections shown before a human saves or applies a revision.
#[derive(Debug, Clone, Serialize)]
pub struct Effective {
    /// Complete validated setup, independent of a running instance.
    pub definition: Definition,
    /// Human-selected runtime instance slug.
    pub instance_name: String,
    /// Explicit owner reference, absent when the selected account default is missing.
    pub runtime_model_ref: Option<String>,
    /// Immutable source revision captured when this draft was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    /// Current setup revision of the fleet being improved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_setup_revision: Option<String>,
}

/// Readiness observations do not mutate the authored requirements.
#[derive(Debug, Clone, Serialize)]
pub struct Review {
    /// The complete setup this review binds.
    pub effective: Effective,
    /// Immutable declarations derived from that setup.
    pub requirements: Requirements,
    /// Live checks, with bounded server-owned recovery actions.
    pub readiness: Vec<Check>,
    /// Digest of effective content and access/model resolution versions.
    pub review_digest: String,
    /// Versioned policy interpretation.
    pub policy_semantics: String,
    /// Observation time excluded from the digest.
    #[serde(with = "afd_wire::instant")]
    pub observed_at: jiff::Timestamp,
}

impl Review {
    /// Requires the human's observed digest before a setup mutation proceeds.
    ///
    /// # Errors
    /// Returns the fresh digest on effective setup or authority drift.
    pub fn require_digest(&self, expected: &str) -> crate::Result<()> {
        if expected != self.review_digest {
            return Err(crate::error::ErrorKind::Review {
                digest: self.review_digest.clone(),
            }
            .into());
        }
        Ok(())
    }
}
