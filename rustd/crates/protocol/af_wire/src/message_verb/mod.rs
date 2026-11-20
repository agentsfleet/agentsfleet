//! The runner's messages verb: a fleet saying one line to the thread it was
//! asked from, before it answers.
//!
//! The daemon holds the channel credential and posts; the runner sends text.
//! Neither end bounds its length or how many a run sends: the channel's own
//! limit refuses what it cannot take, and the post answers `delivered: false`.

use std::borrow::Cow;
use std::time::Duration;

use af_validate::nul_free;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// The longest the daemon spends posting one message, every retry included,
/// before it answers `delivered: false`.
///
/// Declared beside the request because both ends depend on it: the runner's
/// own call timeout must outlast it, or a slow channel reads to the model as a
/// daemon that never answered, and a retried tool call posts the line twice.
/// The runner pins that ordering at compile time.
pub const MESSAGE_DELIVERY_DEADLINE: Duration = Duration::from_secs(12);

/// `POST /v1/runners/me/runs/{run_id}/messages` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MessageRequest<'a> {
    /// The run's fencing token; a holder the fleet has superseded is refused.
    #[garde(skip)]
    pub fencing_token: u64,
    /// What to say in the thread: nonempty, with no NUL character.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_length = 1))]
    #[garde(length(bytes, min = 1), custom(nul_free))]
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
mod tests;
