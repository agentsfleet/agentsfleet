//! Reading the narrative log: one fleet's history, one workspace's, one event.
//!
//! # One text per scope and cursor, because a cached plan is generic
//!
//! sqlx prepares each text once per connection, and after five runs Postgres
//! may plan it once for every value — a generic plan. That plan cannot decide
//! a guard like `($2 IS NULL OR fleet_id = $2)`, so everything behind one
//! becomes a filter. When the scope, the cursor and `since` were NULL-gated on
//! one listing text, a generic plan scanned the workspace's whole history from
//! the newest row down to the cursor, a page deeper each time.
//!
//! So nothing on the page path is gated. The scope, the cursor and the actor
//! filter pick the text: fleet or workspace, first page or resumed, filtered
//! or not. `since` is bound on every listing as its two slots — an instant or
//! a span, NULL when absent — and the statement resolves them on its own clock
//! (`statement::` explains the expression). The actor gets its own texts rather than a guard because a guard's row estimate
//! is tiny whatever it binds, and a plan expecting one row sorts instead of
//! walking the index to its `LIMIT`. The column list, ordering and limit still
//! come from one vocabulary, so the variants cannot drift in what they select.
//!
//! # Newest-first, and the tie-break is not decoration
//!
//! `ORDER BY created_at DESC, turn_id DESC`, and the cursor compares the PAIR
//! (`(created_at, turn_id) < ($1, $2)`). Ordering on the timestamp alone would
//! drop or repeat rows whenever two events share a millisecond, which under a
//! webhook burst is most of them.

mod actor;
mod cursor;
mod detail;
mod filter;
mod filtered;
mod row;
pub(crate) mod statement;
mod tool_call;

use af_core::id::Uuid7;
use af_core::turn::status;
use afd_db::Db;
use jiff_sqlx::ToSqlx as _;

use crate::view::error::{self, Result};

use self::statement::{
    SELECT_DETAIL, SELECT_THREAD_FINISHED_BEFORE, SELECT_THREAD_PAGE, SELECT_THREAD_PAGE_AFTER,
    listing_text,
};

pub use self::cursor::Cursor;
pub use self::detail::TurnDetailRow;
pub use self::filter::{Filter, Since, WIDEST_WINDOW, glob_to_like, parse_since, prefix_to_like};
pub(crate) use self::row::Stored;
pub use self::row::TurnRow;
pub use self::tool_call::{CallAddress, ToolCallRow};

/// What each read was doing, for the operator's log line.
const CONTEXT_FLEET_PAGE: &str = "read a fleet's history";
const CONTEXT_WORKSPACE_PAGE: &str = "read a workspace's history";
const CONTEXT_DETAIL: &str = "read one event";
const CONTEXT_STOP: &str = "stop a fleet turn";
const CONTEXT_THREAD: &str = "read a fleet's message thread";
const CONTEXT_FINISHED: &str = "read a fleet's finished turns";

/// The page a caller gets when they name no size.
pub const DEFAULT_LIMIT: i64 = 50;

/// The page a caller of the message thread gets when they name no size.
pub const THREAD_DEFAULT_LIMIT: i64 = 25;

/// The largest message-thread page this surface will build. Every row
/// carries a trigger payload and an agent's full answer, so the page is also
/// cut to its byte budget.
pub const THREAD_MAX_LIMIT: i64 = 100;

/// The largest page this surface will build.
///
/// `LIMIT_MAX`, mirrored. The ceiling is the correlated cost subselect's bound
/// as much as the payload's: it executes once per returned row.
pub const MAX_LIMIT: i64 = 200;

/// The operator's read side of a fleet's opened `thread.turns` events.
#[derive(Debug, Clone)]
pub struct History {
    database: Db,
}

impl History {
    /// Reads through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// A whole workspace's history, newest first, optionally one fleet of it.
    ///
    /// `fleet` narrows the read to one fleet: the console's Live Wall drill-down
    /// and a fleet's own list are the same read.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a row this build cannot
    /// read. The cursor and the window were already resolved by the caller, so
    /// nothing here is the caller's fault.
    pub async fn page_for_workspace(
        &self,
        workspace: &Uuid7,
        fleet: Option<&Uuid7>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> Result<Vec<TurnRow>> {
        let context = if fleet.is_some() {
            CONTEXT_FLEET_PAGE
        } else {
            CONTEXT_WORKSPACE_PAGE
        };
        self.page(workspace, fleet, filter, cursor, limit, context)
            .await
    }

    /// One event, bodies included, or nothing.
    ///
    /// `Result<Option<_>>`: a row that is not there is an ANSWER, and a
    /// datastore that would not say is a failure. Collapsing the two would make
    /// an outage look like a deleted event to every caller.
    ///
    /// This is the only read that carries `request_json` and `response_text`.
    /// A listing is asked for up to two hundred rows and would pay for both on
    /// every one of them; an expanded row is asked for one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn detail(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        turn_id: &Uuid7,
    ) -> Result<Option<TurnDetailRow>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(SELECT_DETAIL)
            .bind(workspace)
            .bind(fleet)
            .bind(turn_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_DETAIL))?;
        found.as_ref().map(TurnDetailRow::read).transpose()
    }

    /// Stops `turn_id` of `fleet` in `workspace`, in its own transaction:
    /// `None` when the workspace holds no such turn on that fleet. The runs it
    /// marked are the caller's to announce after this commits.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn stop(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        turn_id: &Uuid7,
    ) -> Result<Option<crate::view::stop::Stopped>> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Connection::begin(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_STOP))?;
        let stopped = crate::view::stop::stop_on(&mut transaction, workspace, fleet, turn_id)
            .await
            .map_err(error::query(CONTEXT_STOP))?;
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_STOP))?;
        Ok(stopped)
    }

    /// One page of a fleet's chat thread, newest first, bodies included.
    ///
    /// The caller asks for one row MORE than it will serve: whether a next
    /// page exists is then a fact rather than a guess, which is what lets the
    /// byte budget above it cut a page short and still hand back an honest
    /// cursor.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a row this build cannot
    /// read.
    pub async fn thread_page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> Result<Vec<TurnDetailRow>> {
        let scoped = cursor.map_or_else(
            || sqlx::query(SELECT_THREAD_PAGE).bind(workspace).bind(fleet),
            |at| {
                sqlx::query(SELECT_THREAD_PAGE_AFTER)
                    .bind(workspace)
                    .bind(fleet)
                    .bind(at.created_at.to_sqlx())
                    .bind(&at.turn_id)
            },
        );
        let bound = limit.clamp(1, THREAD_MAX_LIMIT + 1);
        let mut connection = self.database.acquire().await?;
        let rows = scoped
            .bind(bound)
            .bind(
                &[
                    crate::Producer::Steer.as_str(),
                    crate::Producer::ScheduleFire.as_str(),
                ][..],
            )
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_THREAD))?;
        rows.iter().map(TurnDetailRow::read).collect()
    }

    /// The finished rows of a fleet's thread before `at`, newest first, bodies
    /// included: what a chat run reads its turns from. Finished is the
    /// statement's predicate, so `limit` counts finished rows only, and no
    /// waiting message is merged in, since it has no answer yet.
    ///
    /// # Errors
    /// As [`Self::thread_page`].
    pub async fn finished_before(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> Result<Vec<TurnDetailRow>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(SELECT_THREAD_FINISHED_BEFORE)
            .bind(workspace)
            .bind(fleet)
            .bind(at.created_at.to_sqlx())
            .bind(&at.turn_id)
            .bind(status::PROCESSED)
            .bind(status::FLEET_ERROR)
            .bind(limit.clamp(1, THREAD_MAX_LIMIT + 1))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_FINISHED))?
            .iter()
            .map(TurnDetailRow::read)
            .collect()
    }

    /// The listing both entry points run: the text their scope, cursor and
    /// actor filter pick, bound in the order that text numbers them.
    async fn page(
        &self,
        workspace: &Uuid7,
        fleet: Option<&Uuid7>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
        context: &'static str,
    ) -> Result<Vec<TurnRow>> {
        let actor = filter.actor_like.as_deref();
        let text = listing_text(fleet.is_some(), cursor.is_some(), actor.is_some());
        let query = sqlx::query(text).bind(workspace);
        let query = match fleet {
            Some(fleet) => query.bind(fleet),
            None => query,
        };
        let query = match cursor {
            Some(at) => query.bind(at.created_at.to_sqlx()).bind(&at.turn_id),
            None => query,
        };
        let query = match actor {
            Some(actor) => query.bind(actor),
            None => query,
        };
        let mut connection = self.database.acquire().await?;
        let rows = query
            .bind(Since::instant(filter.since).map(jiff_sqlx::ToSqlx::to_sqlx))
            .bind(Since::span(filter.since))
            .bind(limit.clamp(1, MAX_LIMIT))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(context))?;

        rows.iter().map(TurnRow::read).collect()
    }
}

/// The cursor a page hands back, or nothing when the page is the last one.
///
/// A short page means there is nothing after it, so the cursor is `None` and
/// the client stops. A FULL page yields a cursor even when the next one turns
/// out to be empty — the alternative is a second count query per page to find
/// out, which is a round trip spent to save a client one.
#[must_use]
pub fn next_cursor(page: &[TurnRow], limit: i64) -> Option<Cursor> {
    let last = page.last()?;
    if i64::try_from(page.len()).is_ok_and(|len| len < limit) {
        return None;
    }
    Some(Cursor::after(last.created_at, &last.turn_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event a test row carries, one per ordinal.
    fn event(ordinal: u8) -> af_core::error::Result<Uuid7> {
        Uuid7::parse(&format!("0199a0b0-0000-7000-8000-{ordinal:012x}"))
    }

    fn row_at(second: i64, turn_id: &Uuid7) -> TurnRow {
        let created_at = jiff::Timestamp::constant(second, 0);
        TurnRow {
            fleet_id: turn_id.to_owned(),
            turn_id: turn_id.to_owned(),
            workspace_id: turn_id.to_owned(),
            actor: String::new(),
            event_type: String::new(),
            status: String::new(),
            tokens: None,
            wall_ms: None,
            failure_label: None,
            failure_detail: None,
            resumes_turn_id: None,
            created_at,
            updated_at: created_at,
            cost_nanos: None,
        }
    }

    #[test]
    fn an_empty_page_ends_the_walk() {
        assert!(next_cursor(&[], DEFAULT_LIMIT).is_none());
    }

    #[test]
    fn a_short_page_ends_the_walk() -> af_core::error::Result<()> {
        let page = vec![row_at(10, &event(1)?), row_at(9, &event(2)?)];
        assert!(next_cursor(&page, DEFAULT_LIMIT).is_none());
        Ok(())
    }

    #[test]
    fn a_full_page_resumes_from_its_last_row() -> af_core::error::Result<()> {
        let last = event(2)?;
        let page = vec![row_at(10, &event(1)?), row_at(9, &last)];
        assert_eq!(
            next_cursor(&page, 2),
            Some(Cursor::after(jiff::Timestamp::constant(9, 0), &last))
        );
        Ok(())
    }
}
