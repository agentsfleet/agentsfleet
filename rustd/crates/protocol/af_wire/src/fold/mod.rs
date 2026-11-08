//! A live run folding in the messages its fleet receives while it works.
//!
//! Before each model call a run asks for every message its fleet has received
//! after the one that started the run and not yet settled, whatever sent it:
//! a steer, a webhook, a chat mention or a schedule fire. The daemon claims
//! them for the run's lease, oldest first, and stops at the first message that
//! is parked behind an approval gate or bound to a durable task, so no message
//! passes one received before it. A run that belongs to a durable task claims
//! nothing here. The report then says how far the run read, and each message
//! it read settles with the run's answer as its reply.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The most messages one claim takes: a burst is read in batches, oldest first.
pub const FOLDS_PER_CLAIM_MAX: u16 = 32;

/// `POST /v1/runners/me/leases/{lease_id}/folds` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FoldRequest {
    /// The lease's fencing token; a holder the fleet has superseded claims
    /// nothing.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The last message's sequence this run already read; 0 before the first.
    #[garde(range(min = 0))]
    pub after_sequence: i64,
}

/// One message the fleet received, claimed for the run, whatever sent it: a
/// steer, a webhook, a chat mention or a schedule fire.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldedInput<'a> {
    /// The message's event id; its own reply lands on that event.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Its place in the fleet's order.
    pub sequence: i64,
    /// The message's text. It is the request's `message` string when it has
    /// one. Otherwise it is the whole request, so a payload with no `message`
    /// still reaches the run verbatim.
    #[serde(borrow)]
    pub text: Cow<'a, str>,
}

/// `POST /v1/runners/me/leases/{lease_id}/folds` reply: the messages claimed
/// for the run, oldest first, at most [`FOLDS_PER_CLAIM_MAX`].
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldResponse<'a> {
    /// The messages, oldest first.
    #[serde(borrow)]
    pub inputs: Vec<FoldedInput<'a>>,
}

#[cfg(test)]
mod tests;
