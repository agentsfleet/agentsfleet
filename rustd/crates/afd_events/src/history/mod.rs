//! Reading the narrative log: one fleet's history, one workspace's, one event.
//!
//! # One statement, not eight
//!
//! `fleet_events_store.zig` concatenates its WHERE clause from the filters that
//! are present, which gives it four statement variants per entry point and
//! eight across the two. Every one of them repeats the same column list, the
//! same ordering and the same limit, and a fix applied to three of four is the
//! failure mode that shape invites.
//!
//! Here the filters are NULL-gated bindings on one statement: an absent filter
//! binds `NULL`, and the guard `($n::type IS NULL OR <predicate>)` collapses.
//! Postgres plans it the same way, and there is one text to get right.
//!
//! NULL rather than an empty-string sentinel — which is what the approval
//! inbox's page uses — because one of these filters is a TIMESTAMP. Zero is a
//! legitimate `created_at`, so a sentinel there would make "since the epoch"
//! indistinguishable from "no lower bound".
//!
//! # Newest-first, and the tie-break is not decoration
//!
//! `ORDER BY created_at DESC, event_id DESC`, and the cursor compares the PAIR
//! (`(created_at, event_id) < ($1, $2)`). Ordering on the timestamp alone would
//! drop or repeat rows whenever two events share a millisecond, which under a
//! webhook burst is most of them.

mod cursor;
mod detail;
mod filter;
mod row;
mod statement;

use afd_core::id::Uuid7;
use afd_db::Db;

use crate::error::{self, Result};

use self::statement::{SELECT_DETAIL, SELECT_PAGE, SELECT_THREAD_PAGE};

pub use self::cursor::Cursor;
pub use self::detail::EventDetailRow;
pub use self::filter::{Filter, glob_to_like, parse_since, prefix_to_like};
pub use self::row::EventRow;

/// What each read was doing, for the operator's log line.
const CONTEXT_FLEET_PAGE: &str = "read a fleet's history";
const CONTEXT_WORKSPACE_PAGE: &str = "read a workspace's history";
const CONTEXT_DETAIL: &str = "read one event";
const CONTEXT_THREAD: &str = "read a fleet's message thread";

/// The page a caller gets when they name no size.
pub const DEFAULT_LIMIT: i64 = 50;

/// The page a caller of the message thread gets when they name no size.
pub const THREAD_DEFAULT_LIMIT: i64 = 20;

/// The largest message-thread page this surface will build.
///
/// `LIMIT_MAX` from `messages_list.zig`, and deliberately an order of
/// magnitude below [`MAX_LIMIT`]: every row here carries a trigger payload and
/// an agent's full answer, where a listing row carries neither.
pub const THREAD_MAX_LIMIT: i64 = 25;

/// The largest page this surface will build.
///
/// `LIMIT_MAX`, mirrored. The ceiling is the correlated cost subselect's bound
/// as much as the payload's: it executes once per returned row.
pub const MAX_LIMIT: i64 = 200;

/// The operator's read side of `core.fleet_events`.
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

    /// One fleet's history, newest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a row this build cannot
    /// read. The cursor and the window were already resolved by the caller, so
    /// nothing here is the caller's fault.
    pub async fn page_for_fleet(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> Result<Vec<EventRow>> {
        self.page(
            workspace,
            Some(fleet.as_str()),
            filter,
            cursor,
            limit,
            CONTEXT_FLEET_PAGE,
        )
        .await
    }

    /// A whole workspace's history, newest first, optionally one fleet of it.
    ///
    /// `fleet` is the drill-down the console's Live Wall uses. It binds the
    /// same argument the per-fleet entry point binds, so the two cannot answer
    /// differently for one fleet.
    ///
    /// # Errors
    /// As [`Self::page_for_fleet`].
    pub async fn page_for_workspace(
        &self,
        workspace: &Uuid7,
        fleet: Option<&Uuid7>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> Result<Vec<EventRow>> {
        self.page(
            workspace,
            fleet.map(Uuid7::as_str),
            filter,
            cursor,
            limit,
            CONTEXT_WORKSPACE_PAGE,
        )
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
        event_id: &str,
    ) -> Result<Option<EventDetailRow>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(SELECT_DETAIL)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(event_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_DETAIL))?;
        found.as_ref().map(EventDetailRow::read).transpose()
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
    ) -> Result<Vec<EventDetailRow>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(SELECT_THREAD_PAGE)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(cursor.map(|at| at.created_at))
            // Bound unconditionally and read only when the guard passes, so
            // the argument count is fixed whether or not a cursor arrived.
            .bind(cursor.map_or("", |at| at.event_id.as_str()))
            .bind(limit.clamp(1, THREAD_MAX_LIMIT + 1))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_THREAD))?;

        rows.iter().map(EventDetailRow::read).collect()
    }

    /// The one statement both listings run.
    async fn page(
        &self,
        workspace: &Uuid7,
        fleet: Option<&str>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
        context: &'static str,
    ) -> Result<Vec<EventRow>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(SELECT_PAGE)
            .bind(workspace.as_str())
            .bind(fleet)
            .bind(cursor.map(|at| at.created_at))
            // Bound unconditionally and read only when the guard above passes,
            // so the argument count is fixed whichever filters are present.
            .bind(cursor.map_or("", |at| at.event_id.as_str()))
            .bind(filter.actor_like.as_deref())
            .bind(filter.since.map(afd_core::clock::UnixMillis::as_millis))
            .bind(limit.clamp(1, MAX_LIMIT))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(context))?;

        rows.iter().map(EventRow::read).collect()
    }
}

/// The cursor a page hands back, or nothing when the page is the last one.
///
/// A short page means there is nothing after it, so the cursor is `None` and
/// the client stops. A FULL page yields a cursor even when the next one turns
/// out to be empty — the alternative is a second count query per page to find
/// out, which is a round trip spent to save a client one.
#[must_use]
pub fn next_cursor(page: &[EventRow], limit: i64) -> Option<Cursor> {
    let last = page.last()?;
    if i64::try_from(page.len()).is_ok_and(|len| len < limit) {
        return None;
    }
    Some(Cursor::after(last.created_at, &last.event_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_at(created_at: i64, event_id: &str) -> EventRow {
        EventRow {
            fleet_id: String::new(),
            event_id: event_id.to_owned(),
            workspace_id: String::new(),
            actor: String::new(),
            event_type: String::new(),
            status: String::new(),
            tokens: None,
            wall_ms: None,
            failure_label: None,
            failure_detail: None,
            checkpoint_id: None,
            resumes_event_id: None,
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
    fn a_short_page_ends_the_walk() {
        let page = vec![row_at(10, "a"), row_at(9, "b")];
        assert!(next_cursor(&page, DEFAULT_LIMIT).is_none());
    }

    #[test]
    fn a_full_page_resumes_from_its_last_row() {
        let page = vec![row_at(10, "a"), row_at(9, "b")];
        assert_eq!(next_cursor(&page, 2), Some(Cursor::after(9, "b")));
    }
}
