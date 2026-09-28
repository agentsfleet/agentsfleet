//! One fleet-stream entry, assembled whole. Split from `event.rs` at the
//! length cap; the field names it writes are `field.rs`.

use super::field;

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
    /// The producer's instant, already rendered as milliseconds.
    pub created_at: &'a str,
}

/// How many fields an entry carries. One number, so a reader counting them and
/// a producer writing them cannot disagree.
pub const ENTRY_FIELD_COUNT: usize = 5;

/// How many fields a QUEUED entry carries: the five, plus the ledger's id.
pub const QUEUED_FIELD_COUNT: usize = ENTRY_FIELD_COUNT + 1;

impl<'a> Entry<'a> {
    /// The field pairs an append writes, in wire order.
    #[must_use]
    pub const fn pairs(&self) -> [(&'static str, &'a str); ENTRY_FIELD_COUNT] {
        [
            (field::ACTOR, self.actor),
            (field::EVENT_TYPE, self.event_type),
            (field::WORKSPACE_ID, self.workspace_id),
            (field::REQUEST_JSON, self.request_json),
            (field::CREATED_AT, self.created_at),
        ]
    }

    /// The field pairs the ledger's append writes: [`Self::pairs`] plus the
    /// logical id, last.
    ///
    /// Only the ledger calls this. A producer holds no id of its own — the id
    /// is the ledger row's — so an entry appended by anything else would be
    /// one the reader refuses for want of this field, which is the intended
    /// outcome: nothing reaches a runner without being admitted first.
    #[must_use]
    pub const fn queued_pairs(
        &self,
        event_id: &'a str,
    ) -> [(&'static str, &'a str); QUEUED_FIELD_COUNT] {
        [
            (field::ACTOR, self.actor),
            (field::EVENT_TYPE, self.event_type),
            (field::WORKSPACE_ID, self.workspace_id),
            (field::REQUEST_JSON, self.request_json),
            (field::CREATED_AT, self.created_at),
            (field::EVENT_ID, event_id),
        ]
    }
}
