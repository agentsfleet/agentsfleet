//! The turns a chat run carries: every finished turn of the fleet's thread
//! before the event, each with the tool calls its run made, as the model
//! will read them.
//!
//! Read through [`Thread`], which [`Conversations`] implements with keyset
//! pages over the thread's finished rows and one read of each turn's calls,
//! so a read that fails is tested without a database. A read that fails
//! issues the run with no turns, because a follow-up answered without its
//! context beats one refused.
//!
//! No count, text or byte cap applies: a run carries its whole conversation,
//! and the model's window is the in-run compaction's (`af_context`). The run
//! frame ([`MESSAGE_BYTES_MAX`]) is the one bound, a transport limit: the read
//! stops at the turn whose calls pass one frame, so it never holds more than a
//! frame and that turn, and [`fit_frame`] drops an assignment's oldest turns,
//! then empties its oldest finished calls, until it fits one, and logs it.

use std::borrow::Cow;
use std::fmt::Debug;

use af_core::id::Uuid7;
use af_core::turn::status;
use af_observability::metrics::label::fleet::HistoryCut;
use af_observability::producers::fleet::history as metrics;
use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::run::{ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, EarlierCall, RunPayload, Turn};
use af_wire::tool_detail::ItemStatus;
use af_wire::tool_trace::fenced_call_id;
use af_wire::turn::{TurnKind, message_of};
use afd_db::Db;
use afd_turns::view::Cursor;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::error::query;
use crate::run::envelope::Acquired;
use crate::run::sql::history::{SELECT_CALLS_OF, SELECT_FINISHED_BEFORE};

/// The event a chat run issued without its turns is logged under.
pub(super) const EVENT_HISTORY_UNAVAILABLE: &str = "run_history_unavailable";
/// The event an assignment whose oldest turns the frame cut is logged under.
pub(super) const EVENT_HISTORY_CUT: &str = "run_history_cut_to_frame";
/// What a finished call the frame emptied answers, in place of its output.
const CALL_CUT: &str = "[cut to fit the run frame]";
/// The finished turns one read takes, unless `[turns] history_page_size` says.
///
/// A bound on one statement's cost, not on the history, which is read page
/// after page until the thread or the frame runs out.
pub const HISTORY_PAGE_TURNS: i64 = 100;
/// Room the run frame's envelope takes around its payload: the message's
/// `type` and the run's `kind` tags. Measured in this module's tests.
pub(crate) const FRAME_ENVELOPE_BYTES: usize = 64;

/// One finished turn of a fleet's thread, as the history read selects it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FinishedTurn {
    /// The turn.
    pub turn_id: Uuid7,
    /// When it came in: half of the cursor the next page reads before.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: Timestamp,
    /// Its place in the thread, which a saved summary's watermark counts in.
    pub sequence: i64,
    /// How it ended: processed or failed.
    pub status: String,
    /// The failure a failed turn ended on.
    pub failure_reason: Option<String>,
    /// The trigger payload as stored, serialized to text.
    pub request_json: String,
    /// The fleet's answer; none from a run that left no reply.
    pub answer_text: Option<String>,
}

/// One tool call a turn's run made, as the history read selects it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CallRow {
    /// The turn newest when the call's step began.
    pub turn_id: Uuid7,
    /// The fence of the run that made it.
    pub fencing_token: i64,
    /// Its number in that run.
    pub sequence: i64,
    /// The tool it called.
    pub tool: String,
    /// How it ended, as `af_wire::tool_detail::ItemStatus` spells it.
    pub status: String,
    /// Its arguments, serialized to text.
    pub arguments: String,
    /// What it returned.
    pub output: String,
}

impl FinishedTurn {
    /// The bytes of text the row stored: what the read counts toward one frame.
    fn stored_bytes(&self) -> usize {
        self.request_json.len() + self.answer_text.as_ref().map_or(0, String::len)
    }
}

impl CallRow {
    /// The bytes of text the call stored.
    const fn stored_bytes(&self) -> usize {
        self.tool.len() + self.arguments.len() + self.output.len()
    }
}

/// A fleet's chat thread, read before one event.
#[async_trait::async_trait]
pub trait Thread: Send + Sync + Debug {
    /// Up to `limit` of `fleet`'s finished turns older than `at`, newest
    /// first. Finished is the read's own predicate, so a running or refused
    /// row never takes a place a turn could fill.
    ///
    /// # Errors
    /// A datastore that would not answer, or a row this build cannot read.
    async fn finished_before(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> crate::Result<Vec<FinishedTurn>>;

    /// Every tool call `fleet`'s runs made for `turn`, in the order they
    /// were made.
    ///
    /// # Errors
    /// As [`Self::finished_before`].
    async fn calls_of(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        turn: &Uuid7,
    ) -> crate::Result<Vec<CallRow>>;

    /// The finished turns one [`Self::finished_before`] read asks for.
    fn page_turns(&self) -> i64;
}

/// The fleet threads a chat run's history is read from, in Postgres.
#[derive(Debug, Clone)]
pub struct Conversations {
    database: Db,
    page_turns: i64,
}

impl Conversations {
    /// Reads through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self {
            database,
            page_turns: HISTORY_PAGE_TURNS,
        }
    }

    /// Reads `turns` finished turns per statement.
    #[must_use]
    pub const fn with_page_turns(mut self, turns: i64) -> Self {
        self.page_turns = turns;
        self
    }
}

#[async_trait::async_trait]
impl Thread for Conversations {
    async fn finished_before(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> crate::Result<Vec<FinishedTurn>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as(SELECT_FINISHED_BEFORE)
            .bind(workspace)
            .bind(fleet)
            .bind(at.created_at.to_sqlx())
            .bind(&at.turn_id)
            .bind(status::PROCESSED)
            .bind(status::FLEET_ERROR)
            .bind(limit)
            .fetch_all(&mut *connection)
            .await
            .map_err(query("read a fleet's finished turns"))
    }

    async fn calls_of(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        turn: &Uuid7,
    ) -> crate::Result<Vec<CallRow>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as(SELECT_CALLS_OF)
            .bind(workspace)
            .bind(fleet)
            .bind(turn)
            .fetch_all(&mut *connection)
            .await
            .map_err(query("read a fleet's earlier tool calls"))
    }

    fn page_turns(&self) -> i64 {
        self.page_turns
    }
}

/// The turns `acquired`'s run carries: none unless its event is a chat
/// message, and none when the read fails.
pub(super) async fn turns(
    thread: &dyn Thread,
    acquired: &Acquired,
    event_type: TurnKind,
) -> Vec<Turn<'static>> {
    let at = Cursor {
        created_at: acquired.turn_created_at,
        turn_id: acquired.turn_id.clone(),
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

/// `turns`, for the event at `at` in `workspace`'s `fleet`: what a chat
/// run carries, read through `thread`.
pub async fn turns_before(
    thread: &dyn Thread,
    workspace: &Uuid7,
    fleet: &Uuid7,
    at: &Cursor,
    event_type: TurnKind,
) -> Vec<Turn<'static>> {
    if event_type != TurnKind::Chat {
        return Vec::new();
    }
    // Boxed: the page walk holds a row across each call read, and every
    // issue path awaits this.
    match Box::pin(read(thread, workspace, fleet, at)).await {
        Ok((turns, bytes)) => {
            metrics::carried(bytes);
            turns
        }
        Err(failure) => {
            metrics::read_failed();
            let error_code = failure.code().as_str();
            let fleet_id = fleet.as_str();
            let agentsfleet_turn_id = at.turn_id.as_str();
            let event = EVENT_HISTORY_UNAVAILABLE;
            tracing::warn!(error_code, fleet_id, agentsfleet_turn_id, event);
            Vec::new()
        }
    }
}

/// Every finished turn before `at`, oldest first, with its calls, read a
/// page at a time, newest first, until the thread ends or what was read
/// passes one frame; and the bytes it stored.
///
/// A turn's calls are read with the turn, not with its page: records carry
/// no cap, so one page's calls could hold many frames before a check after
/// the page ran.
async fn read(
    thread: &dyn Thread,
    workspace: &Uuid7,
    fleet: &Uuid7,
    at: &Cursor,
) -> crate::Result<(Vec<Turn<'static>>, usize)> {
    let mut newest_first = Vec::new();
    let mut stored = 0_usize;
    let mut cursor = at.clone();
    let page_turns = thread.page_turns();
    'pages: loop {
        let page = thread
            .finished_before(workspace, fleet, &cursor, page_turns)
            .await?;
        let ended = i64::try_from(page.len()).map_or(true, |read| read < page_turns);
        let Some(oldest) = page.last() else {
            break;
        };
        cursor = Cursor {
            created_at: oldest.created_at,
            turn_id: oldest.turn_id.clone(),
        };
        for row in page {
            let made = thread.calls_of(workspace, fleet, &row.turn_id).await?;
            stored += row.stored_bytes() + made.iter().map(CallRow::stored_bytes).sum::<usize>();
            newest_first.extend(turn(row, made));
            if stored > MESSAGE_BYTES_MAX {
                break 'pages;
            }
        }
        if ended {
            break;
        }
    }
    newest_first.reverse();
    Ok((newest_first, stored))
}

/// One finished row as a turn, carrying `calls`; a row still running,
/// queued or refused at a gate is no turn.
pub(super) fn turn(row: FinishedTurn, calls: Vec<CallRow>) -> Option<Turn<'static>> {
    let answer = match row.status.as_str() {
        status::PROCESSED => row.answer_text.unwrap_or_else(|| ANSWER_NONE.to_owned()),
        status::FLEET_ERROR => {
            let label = row.failure_reason.as_deref().unwrap_or(status::FLEET_ERROR);
            format!("{ANSWER_FAILED}{label}{ANSWER_FAILED_END}")
        }
        _unfinished => return None,
    };
    Some(Turn {
        sequence: u64::try_from(row.sequence).unwrap_or_default(),
        message: Cow::Owned(message_of(&row.request_json).into_owned()),
        calls: calls.into_iter().filter_map(call).collect(),
        answer: Cow::Owned(answer),
    })
}

/// One stored call as the model reads it again; a status this build does
/// not spell is no call, as the item detail refuses it.
pub(super) fn call(row: CallRow) -> Option<EarlierCall<'static>> {
    let status = ItemStatus::parse(&row.status)?;
    // The record verb stores an object; anything else reads as no arguments.
    let arguments = serde_json::from_str(&row.arguments).unwrap_or_default();
    Some(EarlierCall {
        call_id: Cow::Owned(fenced_call_id(row.fencing_token, &row.sequence.to_string())),
        tool: Cow::Owned(row.tool),
        arguments,
        output: Cow::Owned(row.output),
        status,
    })
}

/// `payload` with its oldest turns dropped, then its oldest finished calls'
/// arguments and output, until the assignment fits one run frame.
///
/// The frame is a transport limit, so the cut is logged and counted. A
/// payload still past the frame is left for the gateway, which fails its run
/// as too large.
#[must_use]
pub fn fit_frame(mut payload: RunPayload<'_>) -> RunPayload<'_> {
    let room = MESSAGE_BYTES_MAX.saturating_sub(FRAME_ENVELOPE_BYTES);
    let mut dropped = 0_usize;
    while !payload.history.is_empty() {
        let Ok(total) = af_validate::serialized_len(&payload) else {
            break;
        };
        let Some(mut over) = total.checked_sub(room).filter(|over| *over > 0) else {
            break;
        };
        // Each dropped turn takes its bytes and one comma; drop as many of
        // the oldest as the excess needs, then measure again.
        let take = payload
            .history
            .iter()
            .take_while(|turn| {
                let taking = over > 0;
                over = over.saturating_sub(af_validate::serialized_len(turn).unwrap_or(0) + 1);
                taking
            })
            .count()
            .max(1);
        payload.history.drain(..take.min(payload.history.len()));
        dropped += take;
    }
    // History gone, the oldest finished calls give up their bodies; each
    // keeps its tool and id, so the recovery note still names every call.
    let mut emptied = 0_usize;
    while af_validate::serialized_len(&payload).is_ok_and(|total| total > room) {
        let Some(call) = payload.finished_calls.get_mut(emptied) else {
            break;
        };
        call.output = Cow::Borrowed(CALL_CUT);
        call.arguments.clear();
        emptied += 1;
    }
    if dropped > 0 || emptied > 0 {
        metrics::cut(HistoryCut::Frame);
        let (run_id, fleet_id) = (payload.run_id.as_ref(), payload.event.fleet_id.as_ref());
        let kept = payload.history.len();
        let event = EVENT_HISTORY_CUT;
        tracing::warn!(run_id, fleet_id, dropped, kept, emptied, event);
    }
    payload
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
