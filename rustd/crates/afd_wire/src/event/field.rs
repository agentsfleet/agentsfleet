//! The field names an event carries as a Dragonfly stream entry.
//!
//! Declared here for the reason [`EventType`](super::EventType)'s spellings are: they cross a
//! boundary. A producer writes them and the runner's pull reads them back, so
//! a pair that drifted would make an event one plane wrote one the other
//! cannot recognise — and there are three producers now (the steer, the
//! approval continuation, the repair sweeper), which is two more than a
//! hand-spelled literal survives.

/// Who or what produced the event.
pub const ACTOR: &str = "actor";
/// How the event entered the system.
///
/// The constant is named for the concept and its VALUE is the wire
/// spelling, which are deliberately different words. `event_envelope.zig`
/// shipped `type`, entries written under that name are what a stream can
/// still hold, and a reader is not free to prefer a nicer name — the pair
/// below is the same shape for the same reason.
pub const EVENT_TYPE: &str = "type";
/// The workspace the fleet belongs to.
pub const WORKSPACE_ID: &str = "workspace_id";
/// The trigger payload, carried verbatim. See [`EVENT_TYPE`] on the
/// name/value split.
pub const REQUEST_JSON: &str = "request";
/// The producer's instant, in milliseconds since the epoch.
///
/// Written by the producer rather than derived from the entry id, because
/// the lease path bills against it: a value the ingress stamped is the one
/// a tenant is charged for, and Dragonfly assigning a second opinion at append
/// time would make the charge depend on queue latency.
pub const CREATED_AT: &str = "created_at";
/// The logical event id the admission ledger assigned.
///
/// The entry id Dragonfly mints is a RECEIPT, not an identity: after a
/// replay one logical event can have had two entries, and it is this
/// field — not the entry id — that `core.fleet_events`, the usage ledger
/// and every read address. Written by the ledger's append alone; a
/// producer never spells it.
pub const EVENT_ID: &str = "event_id";
