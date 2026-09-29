//! The operator's steer: the one request body a person writes to a fleet, and
//! the answer it gets. Split from `event.rs` at the length cap.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// `POST /v1/workspaces/{ws}/fleets/{id}/messages` — an operator's steer.
///
/// Unknown fields are refused with 400. A field this endpoint does not read is
/// a typo, or a feature it lacks. Ignoring it would let a client believe a
/// setting took effect.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SteerRequest<'a> {
    /// What to say to the fleet: 1 to 8192 bytes, with no NUL character.
    ///
    /// Bounded on the decoded bytes, which is what reaches the stream. The
    /// escaped form a client sends is not what counts against the limit.
    //
    // NUL is refused because the lease casts the stored body to `jsonb`, which
    // cannot hold `\u0000`: accepted, such a message was answered 202 and then
    // failed at lease.
    #[serde(borrow)]
    #[garde(
        length(bytes, min = 1, max = STEER_MESSAGE_MAX_BYTES),
        custom(rules::message_free_of_nul)
    )]
    pub message: Cow<'a, str>,

    /// Your own name for this message, repeated on its retries: 1 to 200 bytes,
    /// with no NUL character.
    ///
    /// Scoped to the fleet. Send the same value again with the same message and
    /// the answer is the first send's `event_id`: one run, one charge. That holds
    /// even if the fleet has stopped or paused since. The same value with a
    /// different message, or from another sender, is refused with 409
    /// `UZ-AGT-016`. Each signed-in person is one sender; a workspace's API keys
    /// are one between them.
    ///
    /// Omit it and every call is a new message, which is what a person pressing
    /// send twice means.
    //
    // A timeout does not prove an operation failed, and a server cannot tell a
    // retried POST from a second press — the bytes are identical — so only the
    // caller can say which it is. The ledger keys the admission
    // `<fleet_id>:<operation_id>` (`afd_events::steer`), so a retry conflicts on
    // `UNIQUE (producer, producer_key)` and is answered from the first row.
    // Optional on purpose: a human in a terminal has no operation to name, and
    // forcing one would make every caller invent a value whose only job is to
    // be unique.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(custom(rules::usable_operation_id)))]
    pub operation_id: Option<Cow<'a, str>>,
}

/// Whether `id` can name an operation: 1 to [`OPERATION_ID_MAX_BYTES`] bytes,
/// none of them NUL.
///
/// NUL is refused here because the ledger stores the id in a Postgres `text`
/// column, which cannot hold it: accepted, it failed the insert as a 500.
#[must_use]
pub fn operation_id_usable(id: &str) -> bool {
    (1..=OPERATION_ID_MAX_BYTES).contains(&id.len()) && !id.contains(NUL)
}

/// The one character Postgres cannot store in `text` or `jsonb`.
const NUL: char = '\0';

/// The garde rules a steer's fields name, under one lint expectation because
/// garde fixes every custom validator's signature the same way.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "garde fixes the custom-validator signature at `fn(&T, &C) -> Result`; the `()` context arrives by reference because the derive passes it that way"
)]
mod rules {
    use super::{NUL, operation_id_usable};

    /// The garde rule over [`operation_id_usable`].
    pub(super) fn usable_operation_id(id: &str, (): &()) -> garde::Result {
        if operation_id_usable(id) {
            Ok(())
        } else {
            Err(garde::Error::new(OPERATION_ID_UNUSABLE))
        }
    }

    /// What garde reports for an unusable operation id.
    const OPERATION_ID_UNUSABLE: &str = "operation id is empty, too long, or holds NUL";

    /// The garde rule refusing a NUL in a steer's message.
    pub(super) fn message_free_of_nul(message: &str, (): &()) -> garde::Result {
        if message.contains(NUL) {
            Err(garde::Error::new(MESSAGE_HOLDS_NUL))
        } else {
            Ok(())
        }
    }

    /// What garde reports for a message holding NUL.
    const MESSAGE_HOLDS_NUL: &str = "message holds NUL";
}

/// The longest client operation identity a steer may carry.
///
/// Generous enough for a UUID, a ULID, a vendor's delivery id or a short
/// composite, and bounded because it is stored per admission and indexed: an
/// unbounded key would let a caller decide how much of the ledger's index one
/// of its retries occupies.
///
/// The `operation_id` description spells this number out, because that
/// description is published and a constant's name means nothing to a client.
pub const OPERATION_ID_MAX_BYTES: usize = 200;

/// The longest thing anyone may say to a fleet in one steer.
///
/// `MAX_MESSAGE_LEN`, mirrored. A steer is a sentence a person typed; past
/// this it is a payload, and the fleet's own trigger surface is where a
/// payload belongs.
///
/// The `message` description spells this number out, for the reason
/// [`OPERATION_ID_MAX_BYTES`] gives.
pub const STEER_MESSAGE_MAX_BYTES: usize = 8192;

/// What a steer returns once agentsfleet accepts the request.
///
/// The response carries the id the run is found under. Filter the live event
/// tail on `event_id` to follow the message you just sent.
// Unknown fields are tolerated here, unlike on the request: this is a reply a
// client reads, and a field a newer daemon adds must not break an older client.
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
    /// Whether an earlier send of the same `operation_id` already admitted this
    /// message. When `true`, `event_id` is that send's event, which may have
    /// run already. `false` when this call admitted it.
    pub replayed: bool,
}
