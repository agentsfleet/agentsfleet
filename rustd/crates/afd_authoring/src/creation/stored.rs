//! Stored setup keeps source identity and immutable content separately from live checks.

use crate::{
    Result,
    draft::value,
    error::raise::{absent, stored},
    review,
    types::{Candidate, InitialStart},
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// The setup a revision was accepted as: its review subject, the candidate it
/// copied and the review a person accepted.
#[derive(Debug, Clone)]
pub(super) struct Accepted {
    pub(super) context: review::Context,
    pub(super) candidate_id: Uuid7,
    pub(super) candidate_revision: String,
    pub(super) canonical_digest: String,
    pub(super) policy_semantics: String,
    pub(super) review_digest: String,
}

/// `fleet.revisions.review_context`: what the revision's columns do not hold.
/// The digests, the candidate id and the review digest are columns.
#[derive(Serialize, Deserialize)]
pub(super) struct Frozen {
    pub(super) context: review::Context,
    pub(super) candidate_revision: String,
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

    /// The `review_context` column this setup is stored under.
    pub(super) fn frozen(&self) -> Result<String> {
        serde_json::to_string(&Frozen {
            context: self.context.clone(),
            candidate_revision: self.candidate_revision.clone(),
        })
        .map_err(stored)
    }
}

#[derive(Debug)]
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
    pub(super) created_at: Timestamp,
    pub(super) updated_at: Timestamp,
    pub(super) events_processed: i64,
    pub(super) budget_used_nanos: i64,
    pub(super) pending_approvals: i64,
    pub(super) memory_reads_workspace: bool,
    pub(super) memory_publishes_workspace: bool,
}

/// [`super::sql::LOCKED`]'s row, by column name.
#[derive(sqlx::FromRow)]
pub(super) struct StoredRow {
    id: String,
    workspace_id: String,
    name: String,
    status: String,
    setup_stream_ready: Option<bool>,
    initial_start: Option<String>,
    resource_version: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
    setup_version: i64,
    definition: String,
    review_context: Option<String>,
    review_digest: Option<String>,
    source_candidate_id: Option<String>,
    canonical_digest: String,
    policy_semantics: String,
    events_processed: i64,
    budget_used_nanos: i64,
    pending_approvals: i64,
}

impl StoredRow {
    /// Every revision this crate writes carries its review, its source and
    /// the initial start, so a revision without them is a stored fault.
    pub(super) fn decode(self) -> Result<Stored> {
        let frozen: Frozen = value(
            self.review_context
                .as_deref()
                .ok_or_else(|| absent("review_context"))?,
        )?;
        let setup_context = Accepted {
            context: frozen.context,
            candidate_id: Uuid7::parse(
                self.source_candidate_id
                    .as_deref()
                    .ok_or_else(|| absent("source_candidate_id"))?,
            )?,
            candidate_revision: frozen.candidate_revision,
            canonical_digest: self.canonical_digest,
            policy_semantics: self.policy_semantics,
            review_digest: self.review_digest.ok_or_else(|| absent("review_digest"))?,
        };
        Ok(Stored {
            id: Uuid7::parse(&self.id)?,
            workspace_id: Uuid7::parse(&self.workspace_id)?,
            name: self.name,
            status: self.status,
            setup_context,
            setup_stream_ready: self.setup_stream_ready,
            initial_start: value(
                self.initial_start
                    .as_deref()
                    .ok_or_else(|| absent("initial_start"))?,
            )?,
            definition: value(&self.definition)?,
            resource_version: self.resource_version,
            setup_version: self.setup_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            events_processed: self.events_processed,
            budget_used_nanos: self.budget_used_nanos,
            pending_approvals: self.pending_approvals,
            memory_reads_workspace: self.memory_reads_workspace,
            memory_publishes_workspace: self.memory_publishes_workspace,
        })
    }
}
