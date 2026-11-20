//! A live run folding in the messages its fleet receives while it works.
//!
//! Before each model call a run asks for every message its fleet has received
//! after the one that started the run and not yet settled, whatever sent it:
//! a steer, a webhook, a chat mention or a schedule fire. The daemon assigns
//! them to the run, oldest first, and stops at the first message that
//! is parked behind an approval gate or bound to a durable task, so no message
//! passes one received before it. A run that belongs to a durable task is
//! assigned nothing here. The report then says how far the run read, and each message
//! it read settles with the run's answer as its reply.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The most messages one assignment takes: a burst is read in batches, oldest
/// first. A reply is also cut to one frame ([`FoldResponse::keep_within`]).
pub const FOLDS_PER_ASSIGNMENT_MAX: u16 = 32;

/// `POST /v1/runners/me/runs/{run_id}/folds` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FoldRequest {
    /// The run's fencing token; a holder the fleet has superseded is
    /// assigned nothing.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The last message's sequence this run already read; 0 before the first.
    #[garde(range(min = 0))]
    pub after_sequence: i64,
}

/// One message the fleet received, assigned to the run, whatever sent it: a
/// steer, a webhook, a chat mention or a schedule fire.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldedInput<'a> {
    /// The message's event id; its own reply lands on that event.
    #[serde(borrow)]
    pub turn_id: Cow<'a, str>,
    /// Its place in the fleet's order.
    pub sequence: i64,
    /// The message's text. It is the request's `message` string when it has
    /// one. Otherwise it is the whole request, so a payload with no `message`
    /// still reaches the run verbatim.
    #[serde(borrow)]
    pub text: Cow<'a, str>,
}

/// `POST /v1/runners/me/runs/{run_id}/folds` reply: the messages assigned
/// to the run, oldest first, at most [`FOLDS_PER_ASSIGNMENT_MAX`].
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldResponse<'a> {
    /// The messages, oldest first.
    #[serde(borrow)]
    pub inputs: Vec<FoldedInput<'a>>,
}

impl FoldResponse<'_> {
    /// Keeps the oldest inputs whose reply serializes within `bytes_max`, and
    /// always the first, so a run is never answered with nothing while
    /// messages wait. The rest stay assigned to the run, and its next fold,
    /// which starts after the last message it read, returns them.
    ///
    /// # Errors
    /// Preserves a serializer failure; the reply is then left whole.
    pub fn keep_within(&mut self, bytes_max: usize) -> Result<(), serde_json::Error> {
        let mut bytes = af_validate::serialized_len(&FoldResponse::default())?;
        let mut kept = 0;
        for input in &self.inputs {
            // Every input after the first adds the comma before it.
            let added = af_validate::serialized_len(input)? + usize::from(kept > 0);
            bytes = bytes.saturating_add(added);
            if kept > 0 && bytes > bytes_max {
                break;
            }
            kept += 1;
        }
        self.inputs.truncate(kept);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
