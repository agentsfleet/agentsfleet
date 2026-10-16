//! Explicit publication inputs and native Library snapshots.

use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use serde::{Deserialize, Serialize};

/// Bounded Library or immutable-history walk.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct LibraryQuery {
    /// Page size, 1..100.
    #[serde(default = "page_size")]
    #[garde(range(min = 1, max = super::MAX_PAGE_SIZE))]
    pub limit: u32,
    /// Continuation bound to this actor, workspace, collection and page size.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = super::MAX_CURSOR_BYTES)))]
    pub starting_after: Option<String>,
}

fn page_size() -> u32 {
    super::DEFAULT_PAGE_SIZE
}

/// History listing excludes instructions and full configuration.
#[derive(Debug, Serialize)]
pub struct RevisionSummary {
    /// Immutable identity.
    pub id: Uuid7,
    /// Decimal entry-local revision.
    pub revision: String,
    /// Content identity.
    pub canonical_digest: String,
    /// UTC milliseconds.
    pub created_at: i64,
}

/// Publishes an idle chat candidate as a new reusable entry.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct PublishLibrary {
    /// Creator-owned current candidate.
    #[garde(skip)]
    pub draft_candidate_id: Uuid7,
    /// Display label of the reusable entry.
    #[garde(length(chars, min = 1, max = super::MAX_TITLE_CHARS), custom(afd_validate::rules::nul_free))]
    pub label: String,
}

/// Appends one explicitly selected entry without changing any fleet.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct PublishRevision {
    /// Creator-owned current candidate.
    #[garde(skip)]
    pub draft_candidate_id: Uuid7,
}

/// Verified receipt for a candidate's explicit Library publication.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Publication {
    /// Reusable entry chosen by the publishing person.
    pub library_id: String,
    /// Immutable published snapshot.
    pub revision_id: Uuid7,
    /// Exact candidate content that was published.
    pub canonical_digest: String,
}

/// Result of an explicit publication, including the current Library version.
#[derive(Debug, Serialize)]
pub struct Published {
    /// Published reusable entry.
    pub library_id: String,
    /// Revision accepted for this operation, including on replay.
    pub revision_id: Uuid7,
    /// Current head, which can advance independently of this receipt.
    pub head_revision_id: Uuid7,
    /// Strong Library version.
    pub etag: String,
}

/// A reusable native snapshot; no transcript or permissions are copied.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryRevision {
    /// Immutable revision identity.
    pub id: Uuid7,
    /// Reusable entry identity.
    pub library_id: String,
    /// Entry-local decimal revision.
    pub revision: String,
    /// Validated authored setup.
    pub definition: Definition,
    /// Canonical content identity.
    pub canonical_digest: String,
    /// Immutable declarative needs.
    pub requirements: super::Requirements,
    /// Pinned policy meaning.
    pub policy_semantics: String,
    /// UTC milliseconds.
    pub created_at: i64,
}

/// Gallery/owned card excludes private text and instructions.
#[derive(Debug, Serialize)]
pub struct LibraryCard {
    /// Reusable entry identity.
    pub id: String,
    /// Human label.
    pub name: String,
    /// Setup description.
    pub description: String,
    /// Audience, platform or tenant.
    pub visibility: String,
    /// UTC milliseconds.
    pub created_at: i64,
    /// Immutable head requirements.
    pub requirements: super::Requirements,
    /// Authored credential reasons; empty when none were supplied.
    pub required_credentials_reasons: serde_json::Value,
    /// Immutable head identity.
    pub head_revision_id: Uuid7,
    /// Strong Library version.
    pub etag: String,
}

/// Inspect combines immutable reusable content with current contextual checks.
#[derive(Debug, Serialize)]
pub struct Library {
    /// Current entry card.
    pub card: LibraryCard,
    /// Selected pinned revision, or current head when none was requested.
    pub head: LibraryRevision,
    /// Current workspace-owner readiness.
    pub review: super::Review,
    /// Current Library version.
    pub etag: String,
}
