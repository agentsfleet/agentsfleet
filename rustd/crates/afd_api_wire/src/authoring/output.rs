//! Private projections; generic Definition values serialize without a JSON copy.

use serde::Serialize;
use serde_json::Value;

/// Saved setup version and its server-assigned identifier. This version cannot change.
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
    /// Highest cancelled message sequence, as a decimal string.
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
    /// Closing timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option", default)]
    pub closed_at: Option<jiff::Timestamp>,
    /// Private retention deadline, such as `2026-10-24T05:00:00.000Z`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option", default)]
    pub expires_at: Option<jiff::Timestamp>,
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
    /// Last mutation.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: jiff::Timestamp,
    /// Private retention deadline.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option", default)]
    pub expires_at: Option<jiff::Timestamp>,
    /// The current candidate's revision; null before the chat proposes one.
    pub candidate_revision: Option<i64>,
    /// The saved fleet's current setup revision; null while no fleet is saved.
    pub setup_revision: Option<i64>,
    /// The Library revision this chat published; null until it publishes.
    pub publication_revision: Option<i64>,
}

/// One user or assistant role in a durable request sequence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Message {
    /// True when this accepted message requests saving after successful completion.
    pub save_requested: bool,
    /// A stable error code when the setup could not be saved. The conversation remains available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_error_code: Option<String>,
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
    /// Server-reported failure reason and suggested recovery action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    /// Acceptance timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Finish timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option", default)]
    pub finished_at: Option<jiff::Timestamp>,
}

/// Safe durable failure; callers cannot supply diagnostics or action links.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Failure {
    /// Server-reported failure reason.
    pub reason: afd_wire::authoring::FailureReason,
    /// Stable central registry code.
    pub error_code: String,
    /// Stable recovery action for the authorized client to render.
    pub recovery_action: String,
    /// Model-stream failure category, without request or response content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

/// One page of results. `total` is null when unavailable; `next_cursor` is null on the last
/// page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct CursorPage<T> {
    /// Current bounded rows.
    pub items: Vec<T>,
    /// Known total, or null.
    pub total: Option<u64>,
    /// Use this value unchanged for the next page, or stop when null.
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

/// Cancellation result, including the latest cancelled message sequence and newly cancelled
/// requests.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Cancellation {
    /// Highest cancelled message sequence, as a decimal string.
    pub cancelled_through: String,
    /// Newly transitioned requests.
    pub stopped: Vec<Stopped>,
}
