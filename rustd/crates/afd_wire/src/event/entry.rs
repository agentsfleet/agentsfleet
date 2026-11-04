//! One fleet-stream entry, assembled whole. Split from `event.rs` at the
//! length cap.

/// Every field a fleet-stream entry carries, assembled in one place.
///
/// The reason this type exists rather than an array spelled at each producer:
/// the reader refuses an entry missing ANY of these, so a producer that writes
/// four of five appends work nothing can lease — silently, because the entry
/// is durable, delivered, and undecodable. That is not hypothetical. It shipped:
/// the producers wrote `event_type`/`request_json` and no `created_at` while
/// the reader asked for `type`/`request`/`created_at`, and every event appended
/// after the cutover was unleasable until this type made the set indivisible.
///
/// Named fields rather than positional arguments, because five strings in a row
/// is a swap waiting to happen and an actor written into the type field is a
/// refusal that names the wrong thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    /// Who or what produced the event.
    pub actor: &'a str,
    /// How the event entered the system — an [`EventType`](super::EventType) spelling.
    pub event_type: &'a str,
    /// The workspace the fleet belongs to.
    pub workspace_id: &'a str,
    /// The trigger payload, already serialized.
    pub request_json: &'a str,
    /// The producer's instant, already rendered as RFC 3339, the shape
    /// [`crate::instant`] reads back.
    pub created_at: &'a str,
}

/// How many fields an entry carries. One number, so a reader counting them and
/// a producer writing them cannot disagree.
pub const ENTRY_FIELD_COUNT: usize = 5;
