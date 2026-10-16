//! One explicit work discriminator shares the existing polling envelope.

use super::LeasePayload;
use serde::{Deserialize, Serialize};

/// Placement alone determines which protocol and authority the worker receives.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedLease<'a, C = serde_json::Value> {
    /// Installed fleet work, with its runtime policy.
    Fleet(#[serde(borrow)] Box<LeasePayload<'a>>),
    /// Private setup work, with bounded daemon model access only.
    Authoring(Box<crate::authoring::Lease<C>>),
}

/// `POST /v1/runners/me/leases` reply. Always `200`.
///
/// `lease` is the work, or null with `retry_after_ms` set when there is none —
/// a backoff hint rather than a `204`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseResponse<'a, C = serde_json::Value> {
    /// The work, when there is any.
    #[serde(borrow)]
    pub lease: Option<TaggedLease<'a, C>>,
    /// How long to wait before asking again, when there is none.
    pub retry_after_ms: Option<u32>,
}
