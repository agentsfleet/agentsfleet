//! The normalized event every producer emits and every consumer reads.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// How an event entered the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// An operator or user message.
    Chat,
    /// An inbound webhook from a connected integration.
    Webhook,
    /// A scheduled trigger.
    Cron,
    /// A re-enqueue continuing an earlier run.
    Continuation,
}

impl EventType {
    /// The type `stored` names, or nothing when it names none.
    ///
    /// The set is CLOSED, and an unrecognised spelling answers `None` rather
    /// than a default. A producer from a newer build can write a type this
    /// daemon has no execution path for; running it as `chat` would execute
    /// the wrong thing, and the caller can end the delivery instead.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            "chat" => Some(Self::Chat),
            "webhook" => Some(Self::Webhook),
            "cron" => Some(Self::Cron),
            "continuation" => Some(Self::Continuation),
            _unknown => None,
        }
    }
}

/// One event on the wire, flat by convention.
///
/// `request_json` is opaque JSON bytes carried verbatim — the runner re-parses
/// it for execution and the read endpoints surface it unchanged, so this layer
/// deliberately does not interpret it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<'a> {
    /// The Redis stream entry identifier, which IS the canonical event id.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// The fleet this event belongs to.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// The workspace the fleet belongs to.
    #[serde(borrow)]
    pub workspace_id: Cow<'a, str>,
    /// Who or what produced the event.
    #[serde(borrow)]
    pub actor: Cow<'a, str>,
    /// How the event entered the system.
    pub event_type: EventType,
    /// Opaque request payload, carried verbatim.
    #[serde(borrow)]
    pub request_json: Cow<'a, str>,
    /// Epoch milliseconds at which the event was created.
    pub created_at: i64,
}
