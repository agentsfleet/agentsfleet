//! The turns a chat lease carries: the fleet's thread before the event, the
//! rows the dashboard shows, as the model will read them.
//!
//! Read through [`Thread`], which [`History`] implements with the thread's own
//! keyset statement, so a read that fails is tested without a database. A read
//! that fails issues the lease with no turns, because a follow-up answered
//! without its context beats one refused.

use std::borrow::Cow;
use std::fmt::Debug;

use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_events::{Cursor, EventDetailRow, History};
use afd_observability::metrics::label::fleet::HistoryCut;
use afd_observability::producers::fleet::history as metrics;
use afd_wire::event::{EventType, message_of};
use afd_wire::lease::{
    ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX,
    TURN_TEXT_BYTES_MAX, Turn,
};

use crate::lease::envelope::Acquired;
use crate::lease::verdict::truncate;

/// The event a chat lease issued without its turns is logged under.
pub(super) const EVENT_HISTORY_UNAVAILABLE: &str = "lease_history_unavailable";

/// A fleet's chat thread, read before one event.
#[async_trait::async_trait]
pub trait Thread: Send + Sync + Debug {
    /// Up to `limit` rows of `fleet`'s thread older than `at`, newest first.
    ///
    /// # Errors
    /// A datastore that would not answer, or a row this build cannot read.
    async fn before(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> afd_events::Result<Vec<EventDetailRow>>;
}

#[async_trait::async_trait]
impl Thread for History {
    async fn before(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> afd_events::Result<Vec<EventDetailRow>> {
        self.thread_page(workspace, fleet, Some(at), limit).await
    }
}

/// The turns `acquired`'s lease carries: none unless its event is a chat
/// message, and none when the read fails.
pub(super) async fn turns(
    thread: &dyn Thread,
    acquired: &Acquired,
    event_type: EventType,
) -> Vec<Turn<'static>> {
    let at = Cursor {
        created_at: acquired.event_created_at.as_millis(),
        event_id: acquired.event_id.clone(),
    };
    turns_before(
        thread,
        &acquired.workspace_id,
        &acquired.fleet_id,
        &at,
        event_type,
    )
    .await
}

/// [`turns`], for the event at `at` in `workspace`'s `fleet`: what a chat
/// lease carries, read through `thread`.
pub async fn turns_before(
    thread: &dyn Thread,
    workspace: &Uuid7,
    fleet: &Uuid7,
    at: &Cursor,
    event_type: EventType,
) -> Vec<Turn<'static>> {
    if event_type != EventType::Chat {
        return Vec::new();
    }
    // One row more than is kept, so a window the cap cut is known as one.
    let limit = i64::try_from(HISTORY_TURNS_MAX + 1).unwrap_or(i64::MAX);
    match thread.before(workspace, fleet, at, limit).await {
        Ok(rows) => {
            let window = within_caps(rows);
            metrics::carried(window.turns.iter().map(size).sum());
            window.cuts.into_iter().for_each(metrics::cut);
            window.turns
        }
        Err(failure) => {
            metrics::read_failed();
            let error_code = failure.code().as_str();
            let fleet_id = fleet.as_str();
            let agentsfleet_event_id = at.event_id.as_str();
            let event = EVENT_HISTORY_UNAVAILABLE;
            tracing::warn!(error_code, fleet_id, agentsfleet_event_id, event);
            Vec::new()
        }
    }
}

/// A chat lease's turns, and every cap that cut them.
#[derive(Debug)]
pub(super) struct Window {
    /// The turns, oldest first.
    pub(super) turns: Vec<Turn<'static>>,
    /// Each cap that cut, once.
    pub(super) cuts: Vec<HistoryCut>,
}

/// The finished rows among `rows`, newest first, as turns oldest first: at
/// most [`HISTORY_TURNS_MAX`], each text cut to [`TURN_TEXT_BYTES_MAX`], and
/// the oldest dropped until the rest fit [`HISTORY_BYTES_MAX`].
pub(super) fn within_caps(rows: Vec<EventDetailRow>) -> Window {
    let mut text_cut = false;
    let mut finished: Vec<Turn<'static>> = rows
        .into_iter()
        .filter_map(|row| turn(row, &mut text_cut))
        .collect();
    let turns_cut = finished.len() > HISTORY_TURNS_MAX;
    finished.truncate(HISTORY_TURNS_MAX);
    finished.reverse();
    let mut total: usize = finished.iter().map(size).sum();
    let bytes_cut = total > HISTORY_BYTES_MAX;
    let turns = finished
        .into_iter()
        .skip_while(|turn| {
            let over = total > HISTORY_BYTES_MAX;
            if over {
                total -= size(turn);
            }
            over
        })
        .collect();
    let cuts = [
        (turns_cut, HistoryCut::Turns),
        (text_cut, HistoryCut::Text),
        (bytes_cut, HistoryCut::Bytes),
    ]
    .into_iter()
    .filter_map(|(cut, cap)| cut.then_some(cap))
    .collect();
    Window { turns, cuts }
}

/// One finished row as a turn; a row still running, queued or refused at a
/// gate is no turn.
fn turn(row: EventDetailRow, text_cut: &mut bool) -> Option<Turn<'static>> {
    let answer = match row.row.status.as_str() {
        status::PROCESSED => row.response_text.unwrap_or_else(|| ANSWER_NONE.to_owned()),
        status::FLEET_ERROR => {
            let label = row
                .row
                .failure_label
                .as_deref()
                .unwrap_or(status::FLEET_ERROR);
            format!("{ANSWER_FAILED}{label}{ANSWER_FAILED_END}")
        }
        _unfinished => return None,
    };
    Some(Turn {
        message: Cow::Owned(cut(&message_of(&row.request_json), text_cut)),
        answer: Cow::Owned(cut(&answer, text_cut)),
    })
}

/// `text` within [`TURN_TEXT_BYTES_MAX`], never splitting a character;
/// `text_cut` is set when it had to be.
fn cut(text: &str, text_cut: &mut bool) -> String {
    let kept = truncate(text, TURN_TEXT_BYTES_MAX);
    *text_cut |= kept.len() < text.len();
    kept.to_owned()
}

/// The bytes a turn spends of [`HISTORY_BYTES_MAX`].
fn size(turn: &Turn<'_>) -> usize {
    turn.message.len() + turn.answer.len()
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
