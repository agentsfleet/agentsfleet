//! The runner's messages verb: a fleet saying one line to the thread it was
//! asked from, before it answers.
//!
//! The daemon holds the channel credential and posts; the runner sends text.
//! The bound on the text and the cap on how many a run may send are declared
//! here so the runner's tool and the daemon's verb refuse the same requests.

use std::borrow::Cow;

use afd_validate::nul_free;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// The longest message one post may carry, in bytes.
///
/// A Slack message renders up to four thousand characters before it folds; an
/// interim line is a status, never the answer, so it is held to that.
pub const MESSAGE_MAX_BYTES: usize = 4096;

/// The most messages one lease may post before it answers.
///
/// A run that needs more than this to say what it is doing is looping, or is
/// being steered by what it read; the thread is a person's, and it is not
/// flooded either way.
pub const MESSAGES_PER_RUN_MAX: u32 = 8;

/// `POST /v1/runners/me/leases/{lease_id}/messages` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MessageRequest<'a> {
    /// The lease's fencing token; a holder the fleet has superseded is refused.
    #[garde(skip)]
    pub fencing_token: u64,
    /// What to say in the thread. At most 4096 bytes.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_length = 1, max_length = 4096))]
    #[garde(length(bytes, min = 1, max = MESSAGE_MAX_BYTES), custom(nul_free))]
    pub text: Cow<'a, str>,
}

/// What posting a message did.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePosted {
    /// Whether the thread has the message. `false` when the channel refused
    /// it or stayed unreachable through every retry; the run goes on, and the
    /// report still carries the answer.
    pub delivered: bool,
}

#[cfg(test)]
#[path = "message_verb/tests.rs"]
mod tests;
