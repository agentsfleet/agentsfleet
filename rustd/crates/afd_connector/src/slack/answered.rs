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
//! It can only SKIP a post, and only on seeing this answer's own marker,
//! posted by this daemon's own bot: any app in a channel can post metadata,
//! and one that copied a marker must not be able to silence an answer. A
//! thread it cannot read — a private channel the grant holds no
//! `groups:history` for, a Slack that is down, a deadline — answers
//! [`Unavailable`], and the caller posts anyway: the answer is owed to a
//! person, and at-least-once is the direction that loses nothing.
//!
//! The read starts shortly before the question was admitted rather than at
//! the thread's first message, so an incident thread hundreds of replies long
//! costs one page, and it runs under [`ANSWER_CHECK_DEADLINE`] rather than the
//! mention's three-second window.

use std::ops::ControlFlow;
use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_crypto::secret::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::replies::{METHOD_CONVERSATIONS_REPLIES, Posted, walk};
use super::{Thread, Unavailable};

/// The metadata event type every answer this daemon posts carries.
pub const ANSWER_EVENT_TYPE: &str = "agentsfleet_answer";

/// The metadata event type every interim line carries.
///
/// Its own type rather than the answer's with a field beside it: a reader that
/// predates interim lines checks the type first, so it can never take a line
/// for the answer and skip the answer it still owes.
pub const INTERIM_EVENT_TYPE: &str = "agentsfleet_interim";

/// How long the whole check may take, every page included.
///
/// Its own bound rather than the mention reader's, which is sized to Slack's
/// three-second delivery window; the outbound worker has no such window. A
/// repeat is this plus the poster's own post deadline, and the pair stays
/// under the ten seconds a shutdown waits for an attempt in flight.
pub const ANSWER_CHECK_DEADLINE: Duration = Duration::from_secs(3);

/// How far before the question's admission the read starts, in seconds.
///
/// The event id's instant is this daemon's clock and a message's `ts` is
/// Slack's; five minutes covers any skew between the two by a wide margin
/// and still spares an old thread every page before it.
const CLOCK_SKEW_SECONDS: i64 = 300;

/// Which owed answer a posted message is: the obligation's own key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerMarker {
    /// The fleet that owes it.
    pub fleet_id: String,
    /// The event it answers.
    pub event_id: String,
    /// Which interim line of the run this is, or `None` for the answer.
    ///
    /// A run may speak before it answers, and every line it says lands in the
    /// same thread under the same fleet and event. A line is stamped with
    /// [`INTERIM_EVENT_TYPE`], so a repeat of the answer never finds one, and
    /// its part tells two lines apart. Absent on the wire for the answer, so
    /// every marker posted before interim lines existed still reads as one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Part>,
}

/// One interim line's place: the lease that said it, and its number there.
///
/// The lease is part of it because the count is the lease's: a reclaimed
/// lease numbers its lines from one again, and its first line must not read
/// as the dead lease's first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    /// The lease that posted the line.
    pub lease_id: String,
    /// Which line of that lease it is, from one.
    pub line: u32,
}

impl AnswerMarker {
    /// The `metadata` argument `chat.postMessage` takes for this message.
    #[must_use]
    pub const fn metadata(&self) -> Stamp<'_> {
        Stamp {
            event_type: self.event_type(),
            event_payload: self,
        }
    }

    /// The stamp's type: an interim line's, or the answer's.
    const fn event_type(&self) -> &'static str {
        if self.part.is_some() {
            INTERIM_EVENT_TYPE
        } else {
            ANSWER_EVENT_TYPE
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
    /// Whether this message is the answer `marker` names, posted by `author`.
    pub(super) fn carries(&self, marker: &AnswerMarker, author: &str) -> bool {
        self.user.as_deref() == Some(author)
            && self.metadata.as_ref().is_some_and(|metadata| {
                metadata.event_type == marker.event_type()
                    && serde_json::from_value::<AnswerMarker>(metadata.event_payload.clone())
                        .is_ok_and(|posted| posted == *marker)
            })
    }
}

/// Whether `thread` already holds the answer `marker` names.
///
/// Only a message `author` posted counts: the bot user the grant recorded.
/// Read through `api_base` ([`super::SLACK_API_BASE`] in a deployment) under
/// [`ANSWER_CHECK_DEADLINE`], starting at [`since`] the marker's event, and
/// stopping at the first page that shows the marker.
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
    author: &str,
) -> Result<bool, Unavailable> {
    let endpoint = format!("{api_base}{METHOD_CONVERSATIONS_REPLIES}");
    let oldest = since(&marker.event_id);
    let mut found = false;
    let read = walk(
        client,
        &endpoint,
        token.expose(),
        thread,
        oldest.as_deref(),
        |posted| {
            if posted.carries(marker, author) {
                found = true;
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    );
    tokio::time::timeout(ANSWER_CHECK_DEADLINE, read)
        .await
        .unwrap_or(Err(Unavailable::Timeout))?;
    Ok(found)
}

/// The Slack timestamp a check of `event_id`'s answer reads from:
/// [`CLOCK_SKEW_SECONDS`] before the instant the id opens with.
///
/// `None` for an id that does not open with milliseconds, which reads the
/// whole thread — slower, never wrong.
fn since(event_id: &str) -> Option<String> {
    let (millis, _sequence) = event_id.split_once('-')?;
    let admitted = UnixMillis::from_millis(millis.parse().ok()?);
    let seconds = admitted
        .as_seconds()
        .saturating_sub(CLOCK_SKEW_SECONDS)
        .max(0);
    Some(format!("{seconds}.000000"))
}

#[cfg(test)]
#[path = "answered/tests.rs"]
mod tests;
