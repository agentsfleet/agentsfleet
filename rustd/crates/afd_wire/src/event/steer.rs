//! The operator's steer: the one request body a person writes to a fleet, and
//! the answer it gets. Split from `event.rs` at the length cap.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// `POST /v1/workspaces/{ws}/fleets/{id}/messages` — an operator's steer.
///
/// Unknown fields are ignored rather than refused, which is what
/// `parseFromSlice(.{ .ignore_unknown_fields = true })` does. A client sending
/// a field this build does not read is not making a mistake it needs telling
/// about.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SteerRequest<'a> {
    /// What to say to the fleet.
    ///
    /// Bounded on the decoded bytes, which is what reaches the stream. The
    /// escaped form a client sends is not what counts against the limit.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = STEER_MESSAGE_MAX_BYTES))]
    pub message: Cow<'a, str>,

    /// Your own name for this message, repeated on its retries.
    ///
    /// Scoped to the fleet. Send the same value again with the same message and
    /// the answer is the first send's `event_id` — one run, one charge — even if
    /// the fleet has stopped or paused since. The same value with a different
    /// message, or from another sender, is refused with 409 `UZ-AGT-016`: each
    /// signed-in person is one sender, and a workspace's API keys are one
    /// between them.
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
    #[garde(inner(length(bytes, min = 1, max = OPERATION_ID_MAX_BYTES)))]
    pub operation_id: Option<Cow<'a, str>>,
}

/// The longest client operation identity a steer may carry.
///
/// Generous enough for a UUID, a ULID, a vendor's delivery id or a short
/// composite, and bounded because it is stored per admission and indexed: an
/// unbounded key would let a caller decide how much of the ledger's index one
/// of its retries occupies.
pub const OPERATION_ID_MAX_BYTES: usize = 200;

/// The longest thing anyone may say to a fleet in one steer.
///
/// `MAX_MESSAGE_LEN`, mirrored. A steer is a sentence a person typed; past
/// this it is a payload, and the fleet's own trigger surface is where a
/// payload belongs.
pub const STEER_MESSAGE_MAX_BYTES: usize = 8192;

/// What a steer returns once agentsfleet accepts the request.
///
/// The response carries the id the run is found under. Filter the live event
/// tail on `event_id` to follow the message you just sent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteerAccepted<'a> {
    /// Always `accepted`. A field rather than an implied 202, because that is
    /// what the daemon this ports writes.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// The canonical event id the steer became.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
}
