//! Shared reusable setup contains no transcript, secret values or permission receipt.

use serde::Serialize;
use serde_json::Value;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// Reusable setup summary, excluding instructions and private chat text.
pub struct Card<Q = Value> {
    /// Entry `UUIDv7`.
    pub id: String,
    /// Human label.
    pub name: String,
    /// Authored description.
    pub description: String,
    /// platform or tenant.
    pub visibility: String,
    /// UTC milliseconds.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Declarative needs of the head revision.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub requirements: Q,
    /// Authored explanation keyed by credential name.
    pub required_credentials_reasons: Value,
    /// Immutable current head `UUIDv7`.
    pub head_revision_id: String,
    /// Strong Library version.
    pub etag: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// Immutable reusable setup selected by its revision identifier.
pub struct Revision<D = Value, Q = Value> {
    /// Revision `UUIDv7`.
    pub id: String,
    /// Entry `UUIDv7`.
    pub library_id: String,
    /// Decimal entry-local revision.
    pub revision: String,
    /// Validated native setup.
    #[cfg_attr(feature = "openapi", schema(value_type = afd_fleet_runtime::definition::DefinitionSchema))]
    pub definition: D,
    /// Canonical content identity.
    pub canonical_digest: String,
    /// Immutable declarative needs.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub requirements: Q,
    /// Pinned policy meaning.
    pub policy_semantics: String,
    /// UTC milliseconds.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// Selected reusable content and readiness for the current workspace.
pub struct Library<D = Value, Q = Value, R = Value> {
    /// Current entry card.
    pub card: Card<Q>,
    /// Explicitly selected revision or current head.
    pub head: Revision<D, Q>,
    /// Contextual current readiness.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub review: R,
    /// Strong Library version.
    pub etag: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// Publication receipt retains the original accepted revision on retry.
pub struct Published {
    /// Entry `UUIDv7`.
    pub library_id: String,
    /// Accepted immutable revision `UUIDv7`.
    pub revision_id: String,
    /// Current head `UUIDv7`.
    pub head_revision_id: String,
    /// Current Library strong tag.
    pub etag: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// History metadata without instructions or full configuration.
pub struct RevisionSummary {
    /// Immutable `UUIDv7`.
    pub id: String,
    /// Decimal entry-local revision.
    pub revision: String,
    /// Content identity.
    pub canonical_digest: String,
    /// UTC milliseconds.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
/// Immutable revision with live contextual readiness.
pub struct RevisionRead<D = Value, Q = Value, R = Value> {
    /// Immutable reusable content.
    pub revision: Revision<D, Q>,
    /// Current owner/workspace readiness.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub review: R,
}
