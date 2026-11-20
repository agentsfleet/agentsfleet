//! The operator's steer: the one request body a person writes to a fleet, and
//! the answer it gets. Split from `turn.rs` at the length cap.

use std::borrow::Cow;

use af_validate::nul_free;
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
    // NUL is refused because the run casts the stored body to `jsonb`, which
    // cannot hold `\u0000`: accepted, such a message was answered 202 and then
    // failed at run.
    #[serde(borrow)]
    #[garde(
        length(bytes, min = 1, max = STEER_MESSAGE_MAX_BYTES),
        custom(nul_free)
    )]
    pub message: Cow<'a, str>,

    /// Your retry identifier, 1 to 200 bytes with no NUL character (U+0000).
    ///
    /// Reuse it with the same message and sender for one run and charge. Reuse with different
    /// text or sender returns 409 `UZ-AGT-016`. Each person is one sender; workspace API keys
    /// share one sender. Omit it to create a new message on every request. Safe retries
    /// remain valid after stopping or pausing the fleet.
    //
    // A timeout does not prove an operation failed, and a server cannot tell a
    // retried POST from a second press — the bytes are identical — so only the
    // caller can say which it is. The ledger keys the turn
    // `<fleet_id>:<operation_id>` (`afd_turns::view::steer`), so a retry conflicts on
    // `UNIQUE (producer, producer_key)` and is answered from the first row.
    // Optional on purpose: a human in a terminal has no operation to name, and
    // forcing one would make every caller invent a value whose only job is to
    // be unique. NUL is refused because the ledger stores the id in a Postgres
    // `text` column, which cannot hold it: accepted, it failed the insert as a
    // 500.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(length(bytes, min = 1, max = OPERATION_ID_MAX_BYTES), custom(nul_free)))]
    pub operation_id: Option<Cow<'a, str>>,
}

/// The longest client operation identity a steer may carry.
///
/// Generous enough for a UUID, a ULID, a vendor's delivery id or a short
/// composite, and bounded because it is stored per turn and indexed: an
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

/// Where an accepted steer went.
///
/// `started`: nothing was waiting and no run was live, so it starts a run.
/// `folded`: the fleet's live run takes it at its next safe boundary.
/// `queued`: it runs after the work ahead of it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteerDisposition {
    /// Starts a run of its own.
    Started,
    /// Joins the fleet's live run.
    Folded,
    /// Waits behind earlier work.
    Queued,
}

/// What a steer returns once agentsfleet accepts the request.
///
/// The response carries the id the run is found under. Filter the live turn
/// tail on `turn_id` to follow the message you just sent.
// Unknown fields are tolerated here, unlike on the request: this is a reply a
// client reads, and a field a newer daemon adds must not break an older client.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteerAccepted<'a> {
    /// Always `accepted`.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// The canonical turn id the steer became.
    #[serde(borrow)]
    pub turn_id: Cow<'a, str>,
    /// Whether an earlier send of the same `operation_id` already admitted this
    /// message. When `true`, `turn_id` is that send's turn, which may have
    /// run already. `false` when this call admitted it.
    pub replayed: bool,
    /// Where the message went.
    pub disposition: SteerDisposition,
    /// The run the message joined, when `disposition` is `folded`.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub folded_into_turn_id: Option<Cow<'a, str>>,
}

/// What a fleet's steer stores as its request: the words the target reads,
/// and which fleet sent them.
///
/// `message` is the sender line, a newline, then `text`, composed once by
/// [`FleetSteerBody::new`]. The model, the chat and the history all read
/// `message` (`message_of`), so the target is told who asked whichever way it
/// reads the turn, and can answer with the same tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetSteerBody<'a> {
    /// The sender line, a newline, then `text`.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// The words the sender wrote, with its secret values masked.
    #[serde(borrow)]
    pub text: Cow<'a, str>,
    /// The fleet that sent it.
    #[serde(borrow)]
    pub sender_fleet_id: Cow<'a, str>,
    /// That fleet's saved name when it sent it.
    #[serde(borrow)]
    pub sender_fleet_name: Cow<'a, str>,
    /// The turn the sender was running when it sent it.
    #[serde(borrow)]
    pub sender_turn_id: Cow<'a, str>,
}

impl<'a> FleetSteerBody<'a> {
    /// The body for `text`, sent by the fleet `sender_fleet_id`, saved as
    /// `sender_fleet_name`, while it ran `sender_turn_id`.
    #[must_use]
    pub fn new(
        text: &'a str,
        sender_fleet_id: &'a str,
        sender_fleet_name: &'a str,
        sender_turn_id: &'a str,
    ) -> Self {
        let line = sender_line(sender_fleet_name, sender_fleet_id);
        Self {
            message: Cow::Owned(format!("{line}\n{text}")),
            text: Cow::Borrowed(text),
            sender_fleet_id: Cow::Borrowed(sender_fleet_id),
            sender_fleet_name: Cow::Borrowed(sender_fleet_name),
            sender_turn_id: Cow::Borrowed(sender_turn_id),
        }
    }
}

/// The first line of a fleet's steer: who sent it, and how to answer.
#[must_use]
pub fn sender_line(name: &str, fleet_id: &str) -> String {
    format!("[from fleet {name}, fleet_id {fleet_id}; answer with fleet_steer to that id]")
}
