//! The normalized event every producer emits and every consumer reads.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// How an event entered the system.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// The field names an event carries as a Redis stream entry.
///
/// Declared here for the reason [`EventType`]'s spellings are: they cross a
/// boundary. A producer writes them and the runner's pull reads them back, so
/// a pair that drifted would make an event one plane wrote one the other
/// cannot recognise — and there are three producers now (the steer, the
/// approval continuation, the repair sweeper), which is two more than a
/// hand-spelled literal survives.
pub mod field {
    /// Who or what produced the event.
    pub const ACTOR: &str = "actor";
    /// How the event entered the system.
    pub const EVENT_TYPE: &str = "event_type";
    /// The workspace the fleet belongs to.
    pub const WORKSPACE_ID: &str = "workspace_id";
    /// The trigger payload, carried verbatim.
    pub const REQUEST_JSON: &str = "request_json";
}

/// One event on the wire, flat by convention.
///
/// `request_json` is opaque JSON carried verbatim. The runner parses it to run
/// the event, and the read endpoints return it unchanged.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<'a> {
    /// The canonical event identifier, the same on every surface that shows
    /// the event.
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
//
// Field order is load-bearing. `events.zig` hands its `EventRow` straight to
// `res.json(value, .{})`, which emits the struct's field set in DECLARATION
// order, and the tenant parity suite pins key set AND order — a reordering that a
// set comparison would call identical is one a client reading ordered columns
// can feel. So this declaration mirrors `fleet_events_store.zig`'s `EventRow`
// field for field.
//
// Every optional stays on the wire as an explicit `null` rather than being
// skipped, which is this crate's rule everywhere and the reason it declares no
// `skip_serializing_if`: the Zig emitter writes `null` for an absent optional,
// so a dropped key would be a byte mismatch against the daemon still serving.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    //
    // `null` when the event recorded no telemetry, which the console renders
    // as UNKNOWN and never as a zero charge — an unbilled run and a free run
    // are different facts. Cost is server truth: a client must not derive it
    // from `tokens`.
    pub cost_nanos: Option<i64>,
}

/// A page of event history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse<'a> {
    /// The events on this page, newest first.
    pub items: Vec<EventSummary<'a>>,
    /// Where the next page resumes, or `null` on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// One event as the expanded view renders it — bodies included.
//
// The sibling of [`EventSummary`], and the two are separate types for the
// reason `fleet_event_detail_store.zig` is a separate file from
// `fleet_events_store.zig`: a page of up to two hundred rows pays for every
// column it selects, and the trigger payload and the agent's full answer are
// wanted one row at a time.
//
// Field order is load-bearing here for the same reason it is on
// [`EventSummary`], and the two bodies sit in the MIDDLE of the field set
// rather than at the end — which is why this cannot be [`EventSummary`] plus
// two fields, in this language or the one it ports.
//
// `request_json` is the stored payload serialized to TEXT and carried as a
// JSON string, not as an embedded object. That is what `res.json` emits for
// the Zig row's `[]u8`, and a client parsing the string a second time is the
// contract already in production.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDetail<'a> {
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
    /// The trigger payload as stored, serialized to text.
    #[serde(borrow)]
    pub request_json: Cow<'a, str>,
    /// The agent's full answer.
    ///
    /// `null` while a run is in flight, and on a run that failed before
    /// producing one.
    #[serde(borrow)]
    pub response_text: Option<Cow<'a, str>>,
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
    //
    // `null` when the event recorded no telemetry — see [`EventSummary`] on
    // why that is never rendered as a zero charge.
    pub cost_nanos: Option<i64>,
}

/// A page of one fleet's chat thread, bodies included.
//
// The sibling of [`EventsResponse`], and it carries [`EventDetail`] where
// that carries [`EventSummary`]: the chat view needs the newest turns WITH
// their payload and answer, and used to fan out one detail request per turn
// to get them.
//
// # `total` is always null, and is on the wire anyway
//
// `messages_list.zig` writes `.total = null` and has never written anything
// else — the count would cost a second statement per page for a number the
// view does not render. Dropping the key would be a byte mismatch against
// the daemon still serving, so it stays, typed as the count it would hold.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadResponse<'a> {
    /// The turns on this page, newest first.
    pub items: Vec<EventDetail<'a>>,
    /// How many turns the thread holds in total. Never populated.
    pub total: Option<u32>,
    /// Where the next page resumes, or `null` when this one ends the walk.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// `POST /v1/workspaces/{ws}/fleets/{id}/messages` — an operator's steer.
///
/// One field, and unknown ones are ignored rather than refused, which is what
/// `parseFromSlice(.{ .ignore_unknown_fields = true })` does. A client sending
/// a field this build does not read is not making a mistake it needs telling
/// about.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct SteerRequest<'a> {
    /// What to say to the fleet.
    ///
    /// Bounded on the decoded bytes, which is what reaches the stream. The
    /// escaped form a client sends is not what counts against the limit.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = STEER_MESSAGE_MAX_BYTES))]
    pub message: Cow<'a, str>,
}

/// The longest thing anyone may say to a fleet in one steer.
///
/// `MAX_MESSAGE_LEN`, mirrored. A steer is a sentence a person typed; past
/// this it is a payload, and the fleet's own trigger surface is where a
/// payload belongs.
pub const STEER_MESSAGE_MAX_BYTES: usize = 8192;

// `event_id` is the stream entry id Redis minted, which IS the canonical
// event id.
/// What a steer returns once agentsfleet accepts the request.
///
/// The response carries the id the run is found under. Filter the live event
/// tail on `event_id` to follow the message you just sent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteerAccepted<'a> {
    /// Always `accepted`. A field rather than an implied 202, because that is
    /// what the daemon this ports writes.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// The canonical event id the steer became.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
}

#[cfg(test)]
#[path = "event/tests.rs"]
mod tests;
