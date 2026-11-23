//! The runner's steer verb: the fleet a run runs putting work on another
//! fleet in its workspace.
//!
//! The text bound is the one ceiling every text shares, a person's steer
//! among them, so a fleet asks no more than a person may. The body names no policy: the target runs under its own setup,
//! and a steer carries words, never authority.

use std::borrow::Cow;

use af_validate::{TEXT_MAX_CHARS, nul_free};
use garde::Validate;
use serde::{Deserialize, Serialize};

/// The length of a fleet identifier as text: a hyphenated UUID.
///
/// The bound `garde` holds before the handler parses the value as a `UUIDv7`,
/// so an oversized field is refused without being read.
pub const FLEET_ID_BYTES: usize = 36;

/// `POST /v1/runners/me/runs/{run_id}/steers` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FleetSteerRequest<'a> {
    /// The run's fencing token; a holder the fleet has superseded is refused.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The fleet to steer: another fleet in the run's workspace.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_length = 36, max_length = 36))]
    #[garde(length(bytes, min = FLEET_ID_BYTES, max = FLEET_ID_BYTES))]
    pub fleet_id: Cow<'a, str>,
    /// What to ask it: 1 to 1,048,576 characters with no NUL character.
    // utoipa reads only a literal here: 1_048_576 is `TEXT_MAX_CHARS`, which the
    // test beside this module pins against the published schema.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_length = 1, max_length = 1_048_576))]
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS), custom(nul_free))]
    pub text: Cow<'a, str>,
}

/// What a fleet's steer was answered with.
// Unknown fields are tolerated: this is a reply a runner reads, and a field a
// newer daemon adds must not break an older runner.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetSteerAccepted<'a> {
    /// The event the steer became on the target fleet.
    #[serde(borrow)]
    pub turn_id: Cow<'a, str>,
    /// Whether an earlier send of the same words from the same event already
    /// admitted it. When `true`, `turn_id` is that send's event.
    pub replayed: bool,
}

#[cfg(test)]
mod tests;
