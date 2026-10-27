//! Bounded editor context; private text is never embedded in a shared fleet response.

use super::{CursorPage, Draft, Message};
use serde::Serialize;
use serde_json::Value;

/// One read for current fleet setup, private conversations and filtered receipts.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct AuthoringContext<F = crate::fleet::FleetResponse, D = Value, R = Value> {
    /// Shared current setup, prerequisites and schedules.
    #[cfg_attr(feature = "openapi", schema(value_type = crate::fleet::FleetResponse))]
    pub fleet: F,
    /// Up to three of this creator's conversations, most recently updated first.
    pub conversations: Vec<Conversation<D, R>>,
    /// Up to 25 recorded activity receipts; runtime bodies are excluded before paging.
    pub activities: CursorPage<SetupActivity>,
    /// More conversations for this creator, workspace and fleet, or null.
    pub next_cursor: Option<String>,
}

/// A current private draft and its recent durable message window.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Conversation<D = Value, R = Value> {
    /// Current creator-private setup and review.
    pub draft: Draft<D, R>,
    /// Up to 25 recent role rows, in sequence order.
    pub messages: Vec<Message>,
    /// Reuse with the draft messages endpoint and limit=25 to reconcile this window.
    pub messages_starting_after: Option<String>,
    /// Older role rows can be read from the draft messages endpoint without a cursor.
    pub truncated: bool,
}

/// Safe durable activity metadata, with no runtime request or response bodies.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct SetupActivity {
    /// Stable event identity.
    pub event_id: String,
    /// Always activity for this resource.
    pub event_type: String,
    /// Recorded receipt status.
    pub status: String,
    /// Safe receipt payload encoded as JSON text.
    pub request_json: String,
    /// When the receipt was written.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
}
