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

/// One event as the operator surface renders it.
///
/// Field order is load-bearing. `events.zig` hands its `EventRow` straight to
/// `res.json(value, .{})`, which emits the struct's field set in DECLARATION
/// order, and the tenant parity suite pins key set AND order — a reordering that a
/// set comparison would call identical is one a client reading ordered columns
/// can feel. So this declaration mirrors `fleet_events_store.zig`'s `EventRow`
/// field for field.
///
/// Every optional stays on the wire as an explicit `null` rather than being
/// skipped, which is this crate's rule everywhere and the reason it declares no
/// `skip_serializing_if`: the Zig emitter writes `null` for an absent optional,
/// so a dropped key would be a byte mismatch against the daemon still serving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSummary<'a> {
    /// The fleet this event belongs to.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// The canonical event identifier — the stream entry id that produced it.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// The workspace the fleet belongs to.
    #[serde(borrow)]
    pub workspace_id: Cow<'a, str>,
    /// Who or what produced the event.
    #[serde(borrow)]
    pub actor: Cow<'a, str>,
    /// How the event entered the system, as stored.
    #[serde(borrow)]
    pub event_type: Cow<'a, str>,
    /// Where the event's run got to, as stored.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// Tokens the run spent, absent until a runner reports.
    pub tokens: Option<i64>,
    /// Wall milliseconds the run took, absent until a runner reports.
    pub wall_ms: Option<i64>,
    /// What refused or failed the run, absent on a clean one.
    #[serde(borrow)]
    pub failure_label: Option<Cow<'a, str>>,
    /// The operator-readable cause line, absent when none was carried.
    #[serde(borrow)]
    pub failure_detail: Option<Cow<'a, str>>,
    /// The session checkpoint this run wrote, when it wrote one.
    #[serde(borrow)]
    pub checkpoint_id: Option<Cow<'a, str>>,
    /// The event this one continues, set on a continuation.
    #[serde(borrow)]
    pub resumes_event_id: Option<Cow<'a, str>>,
    /// Epoch milliseconds the row was created.
    pub created_at: i64,
    /// Epoch milliseconds the row last changed.
    pub updated_at: i64,
    /// What this event actually cost, summed over its telemetry rows.
    ///
    /// `null` when the event recorded no telemetry, which the console renders
    /// as UNKNOWN and never as a zero charge — an unbilled run and a free run
    /// are different facts. Cost is server truth: a client must not derive it
    /// from `tokens`.
    pub cost_nanos: Option<i64>,
}

/// A page of event history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse<'a> {
    /// The events on this page, newest first.
    pub items: Vec<EventSummary<'a>>,
    /// Where the next page resumes, or `null` on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}
