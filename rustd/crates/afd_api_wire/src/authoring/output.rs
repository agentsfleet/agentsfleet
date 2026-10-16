//! Private projections; generic Definition values serialize without a JSON copy.

use serde::Serialize;
use serde_json::Value;

/// Immutable normalized setup version, identified by the daemon.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Candidate<D = Value> {
    /// Candidate identity.
    pub id: String,
    /// Decimal draft-local revision.
    pub revision: String,
    /// Complete validated native Definition.
    #[cfg_attr(feature = "openapi", schema(value_type = afd_fleet_runtime::definition::DefinitionSchema))]
    pub definition: D,
    /// Canonical content identity.
    pub canonical_digest: String,
    /// Pinned interpretation of the policy schema.
    pub policy_semantics: String,
}

/// Creator-visible mutable draft and its latest immutable candidate.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Draft<D = Value, R = Value> {
    /// Private draft identity.
    pub id: String,
    /// editing, saving or closed.
    pub state: String,
    /// Human display title.
    pub title: String,
    /// Strong version for idle edits.
    pub etag: String,
    /// Explicit model for future messages.
    pub authoring_model_ref: String,
    /// Decimal standing cancellation watermark.
    pub cancelled_through: String,
    /// Latest accepted setup, absent before the first candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<Candidate<D>>,
    /// Complete effective setup, immutable requirements and live readiness.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub review: Option<R>,
    /// Related fleet, when this draft targets or created one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_fleet_id: Option<String>,
    /// Explicit publication receipt, never synthesized by Save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication: Option<Publication>,
    /// Closing time in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<i64>,
    /// Private retention deadline in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// The exact reusable snapshot explicitly published by this creator.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Publication {
    /// Reusable entry identity.
    pub library_id: String,
    /// Immutable published snapshot.
    pub revision_id: String,
    /// Content identity that Save can verify and reuse.
    pub canonical_digest: String,
}

/// The message is saved and queued. The model call happens separately.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = DraftMessageAccepted))]
#[derive(Debug, Serialize)]
pub struct Accepted {
    /// Always accepted.
    pub status: String,
    /// Decimal user request sequence.
    pub sequence: String,
    /// Whether this exact request had already committed.
    pub replayed: bool,
}

/// Atomic draft creation response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Created<D = Value, R = Value> {
    /// Created private resource.
    #[serde(flatten)]
    pub draft: Draft<D, R>,
    /// Present only when creation accepted an initial message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<Accepted>,
}

/// Lightweight creator-only collection item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct DraftCard {
    /// Private draft identity.
    pub id: String,
    /// Human display title.
    pub title: String,
    /// editing, saving or closed.
    pub state: String,
    /// Strong resource version.
    pub etag: String,
    /// Related fleet, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_fleet_id: Option<String>,
    /// Last mutation, Unix milliseconds.
    pub updated_at: i64,
    /// Private retention deadline, Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// One user or assistant role in a durable request sequence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Message {
    /// Decimal request sequence.
    pub sequence: String,
    /// Accepted body replay key.
    pub operation_id: String,
    /// user or assistant.
    pub role: String,
    /// Durable queued/running/terminal state.
    pub state: String,
    /// Text, when this role has private content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Accepted assistant proposal, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    /// Provider usage and cumulative runtime charge, when recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<afd_wire::authoring::Usage>,
    /// Daemon-derived bounded failure and recovery action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    /// Acceptance time, Unix milliseconds.
    pub created_at: i64,
    /// Terminal time, Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

/// Safe durable failure; callers cannot supply diagnostics or action links.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Failure {
    /// Bounded daemon-owned reason.
    pub reason: afd_wire::authoring::FailureReason,
    /// Stable central registry code.
    pub error_code: String,
    /// Stable recovery action for the authorized client to render.
    pub recovery_action: String,
}

/// A bounded keyset page; unavailable totals and final cursors are null.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct CursorPage<T> {
    /// Current bounded rows.
    pub items: Vec<T>,
    /// Known total, or null.
    pub total: Option<u64>,
    /// Opaque continuation, or null.
    pub next_cursor: Option<String>,
}

/// One request cancelled by this Stop operation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Stopped {
    /// Decimal request sequence.
    pub sequence: String,
    /// Always cancelled.
    pub state: String,
}

/// Standing watermark and requests newly cancelled by this Stop.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Cancellation {
    /// Decimal standing watermark.
    pub cancelled_through: String,
    /// Newly transitioned requests.
    pub stopped: Vec<Stopped>,
}
