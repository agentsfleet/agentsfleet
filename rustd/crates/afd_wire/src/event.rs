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

/// The stored spellings, declared once (RULE UFS).
///
/// `parse` and [`EventType::as_str`] are inverses, so each word appeared twice
/// — and a pair that drifted would make a type this daemon writes one it cannot
/// read back. Naming them here leaves one edit site per spelling and keeps the
/// two matches beside each other, which is what makes a missing variant a build
/// failure in the file where its reader lives.
const CHAT: &str = "chat";
const WEBHOOK: &str = "webhook";
const CRON: &str = "cron";
const CONTINUATION: &str = "continuation";

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
            CHAT => Some(Self::Chat),
            WEBHOOK => Some(Self::Webhook),
            CRON => Some(Self::Cron),
            CONTINUATION => Some(Self::Continuation),
            _unknown => None,
        }
    }

    /// The word a stored row or a stream field spells this type as.
    ///
    /// The inverse of [`Self::parse`], and deliberately a `match` beside it
    /// rather than a serde round trip: the two are read together, so a variant
    /// added without a spelling fails the build in the same file where its
    /// reader lives.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => CHAT,
            Self::Webhook => WEBHOOK,
            Self::Cron => CRON,
            Self::Continuation => CONTINUATION,
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
