//! Stored setup keeps source identity and immutable content separately from live checks.

use crate::{
    review,
    types::{Candidate, InitialStart},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct Accepted {
    pub(super) context: review::Context,
    pub(super) candidate_id: Uuid7,
    pub(super) candidate_revision: String,
    pub(super) canonical_digest: String,
    pub(super) policy_semantics: String,
    pub(super) review_digest: String,
}

impl Accepted {
    pub(super) fn from_review(
        captured: &super::source::Captured,
        checked: &crate::types::Review,
    ) -> Self {
        Self {
            context: captured.context.clone(),
            candidate_id: captured.candidate.id.clone(),
            candidate_revision: captured.candidate.revision.clone(),
            canonical_digest: captured.candidate.canonical_digest.clone(),
            policy_semantics: captured.candidate.policy_semantics.clone(),
            review_digest: checked.review_digest.clone(),
        }
    }

    pub(super) fn candidate(&self, definition: Definition) -> Candidate {
        Candidate {
            id: self.candidate_id.clone(),
            revision: self.candidate_revision.clone(),
            definition,
            canonical_digest: self.canonical_digest.clone(),
            policy_semantics: self.policy_semantics.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct Stored {
    pub(super) id: Uuid7,
    pub(super) workspace_id: Uuid7,
    pub(super) name: String,
    pub(super) status: String,
    pub(super) setup_context: Accepted,
    pub(super) setup_stream_ready: Option<bool>,
    pub(super) initial_start: InitialStart,
    pub(super) definition: Definition,
    pub(super) resource_version: i64,
    pub(super) setup_version: i64,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) events_processed: i64,
    pub(super) budget_used_nanos: i64,
    pub(super) pending_approvals: i64,
    pub(super) memory_reads_workspace: bool,
    pub(super) memory_publishes_workspace: bool,
}
