//! Whether an answer this daemon owes is already in its thread.
//!
//! `chat.postMessage` has no idempotency key. When Slack takes a post and the
//! reply is lost — a dropped connection, a deadline that fires after Slack
//! wrote the message — the poster reads a transport failure, retries, and the
//! thread gets the answer twice. So every answer is posted carrying a marker,
//! Slack's message metadata naming the fleet and the event it answers, and a
//! poster that is not sure its earlier attempt failed asks the thread first.
//!
//! # What the check can and cannot do
//!
//! It can only SKIP a post, and only on seeing this answer's own marker. A
//! thread it cannot read — a private channel the grant holds no
//! `groups:history` for, a Slack that is down, a deadline — answers
//! [`Unavailable`], and the caller posts anyway: the answer is owed to a
//! person, and at-least-once is the direction that loses nothing.

use std::ops::ControlFlow;

use afd_crypto::secret::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::replies::{METHOD_CONVERSATIONS_REPLIES, Posted, walk};
use super::{READ_DEADLINE, Thread, Unavailable};

/// The metadata event type every answer this daemon posts carries.
pub const ANSWER_EVENT_TYPE: &str = "agentsfleet_answer";

/// Which owed answer a posted message is: the obligation's own key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerMarker {
    /// The fleet that owes it.
    pub fleet_id: String,
    /// The event it answers.
    pub event_id: String,
}

impl AnswerMarker {
    /// The `metadata` argument `chat.postMessage` takes for this answer.
    #[must_use]
    pub const fn metadata(&self) -> Stamp<'_> {
        Stamp {
            event_type: ANSWER_EVENT_TYPE,
            event_payload: self,
        }
    }
}

/// One answer's metadata as it is posted: the marker itself is the payload,
/// so its keys are spelled once, by its own `Serialize`.
#[derive(Debug, Serialize)]
pub struct Stamp<'a> {
    event_type: &'static str,
    event_payload: &'a AnswerMarker,
}

/// Slack's message metadata as a read returns it.
///
/// The payload stays a JSON value, and both fields default, so another app's
/// metadata in the same thread, shaped however that app likes, never fails
/// the page it is on.
#[derive(Debug, Default, Deserialize)]
pub(super) struct MessageMetadata {
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    event_payload: Value,
}

impl Posted {
    /// Whether this message is the answer `marker` names.
    pub(super) fn carries(&self, marker: &AnswerMarker) -> bool {
        self.metadata.as_ref().is_some_and(|metadata| {
            metadata.event_type == ANSWER_EVENT_TYPE
                && serde_json::from_value::<AnswerMarker>(metadata.event_payload.clone())
                    .is_ok_and(|posted| posted == *marker)
        })
    }
}

/// Whether `thread` already holds the answer `marker` names, read through
/// `api_base` — [`super::SLACK_API_BASE`] in a deployment — under
/// [`READ_DEADLINE`].
///
/// Stops at the first page that shows the marker.
///
/// # Errors
/// The [`Unavailable`] reason the thread could not be read for; the caller
/// posts anyway, as the module note says.
pub async fn holds_answer(
    client: &reqwest::Client,
    api_base: &str,
    token: &SecretString,
    thread: &Thread,
    marker: &AnswerMarker,
) -> Result<bool, Unavailable> {
    let endpoint = format!("{api_base}{METHOD_CONVERSATIONS_REPLIES}");
    let mut found = false;
    let read = walk(client, &endpoint, token.expose(), thread, |posted| {
        if posted.carries(marker) {
            found = true;
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    tokio::time::timeout(READ_DEADLINE, read)
        .await
        .unwrap_or(Err(Unavailable::Timeout))?;
    Ok(found)
}

#[cfg(test)]
#[path = "answered/tests.rs"]
mod tests;
