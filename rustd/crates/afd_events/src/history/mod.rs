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
//! or not. `since` is bound on every listing, as `i64::MIN` when absent, which
//! is every `created_at` there is rather than a value a row could hold. The
//! actor gets its own texts rather than a guard because a guard's row estimate
//! is tiny whatever it binds, and a plan expecting one row sorts instead of
//! walking the index to its `LIMIT`. The column list, ordering and limit still
//! come from one vocabulary, so the variants cannot drift in what they select.
//!
//! # Newest-first, and the tie-break is not decoration
//!
//! `ORDER BY created_at DESC, event_id DESC`, and the cursor compares the PAIR
//! (`(created_at, event_id) < ($1, $2)`). Ordering on the timestamp alone would
//! drop or repeat rows whenever two events share a millisecond, which under a
//! webhook burst is most of them.

mod actor;
mod cursor;
mod detail;
mod filter;
mod queued;
mod row;
pub(crate) mod statement;
mod tool_call;

use afd_core::clock::UnixMillis;
use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_db::Db;

use crate::error::{self, Result};

use self::statement::{
    SELECT_DETAIL, SELECT_THREAD_FINISHED_BEFORE, SELECT_THREAD_PAGE, SELECT_THREAD_PAGE_AFTER,
    listing_text,
};

pub use self::cursor::Cursor;
pub use self::detail::EventDetailRow;
pub use self::filter::{Filter, glob_to_like, parse_since, prefix_to_like};
pub use self::row::EventRow;
pub use self::tool_call::{CallAddress, ToolCallRow};

/// What each read was doing, for the operator's log line.
const CONTEXT_FLEET_PAGE: &str = "read a fleet's history";
const CONTEXT_WORKSPACE_PAGE: &str = "read a workspace's history";
const CONTEXT_DETAIL: &str = "read one event";
const CONTEXT_THREAD: &str = "read a fleet's message thread";
const CONTEXT_FINISHED: &str = "read a fleet's finished turns";

/// The page a caller gets when they name no size.
pub const DEFAULT_LIMIT: i64 = 50;

/// The page a caller of the message thread gets when they name no size.
pub const THREAD_DEFAULT_LIMIT: i64 = 20;

/// The largest message-thread page this surface will build.
///
/// Deliberately an order of magnitude below [`MAX_LIMIT`]: every row here carries a trigger payload and
/// an agent's full answer, where a listing row carries neither.
pub const THREAD_MAX_LIMIT: i64 = 25;

/// The `since` a listing binds when the caller named none: the least value a
/// `created_at` can hold, so the bound admits every row and still reaches the
/// index as a condition.
const NO_LOWER_BOUND: i64 = i64::MIN;

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
        let scoped = match cursor {
            None => sqlx::query(SELECT_THREAD_PAGE)
                .bind(workspace.as_str())
                .bind(fleet.as_str()),
            Some(at) => sqlx::query(SELECT_THREAD_PAGE_AFTER)
                .bind(workspace.as_str())
                .bind(fleet.as_str())
                .bind(at.created_at)
                .bind(at.event_id.as_str()),
        };
        let bound = limit.clamp(1, THREAD_MAX_LIMIT + 1);
        let mut connection = self.database.acquire().await?;
        let rows = scoped
            .bind(bound)
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_THREAD))?;
        let delivered = rows
            .iter()
            .map(EventDetailRow::read)
            .collect::<Result<Vec<_>>>()?;
        let waiting = queued::waiting(&mut connection, workspace, fleet, cursor, bound).await?;
        let cut = usize::try_from(bound).unwrap_or(usize::MAX);
        Ok(queued::merged(delivered, waiting, cut))
    }

    /// The finished rows of a fleet's thread before `at`, newest first, bodies
    /// included: what a chat lease reads its turns from. Finished is the
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
    ) -> Result<Vec<EventDetailRow>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(SELECT_THREAD_FINISHED_BEFORE)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(at.created_at)
            .bind(at.event_id.as_str())
            .bind(status::PROCESSED)
            .bind(status::FLEET_ERROR)
            .bind(limit.clamp(1, THREAD_MAX_LIMIT + 1))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_FINISHED))?
            .iter()
            .map(EventDetailRow::read)
            .collect()
    }

    /// The listing both entry points run: the text their scope, cursor and
    /// actor filter pick, bound in the order that text numbers them.
    async fn page(
        &self,
        workspace: &Uuid7,
        fleet: Option<&str>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
        context: &'static str,
    ) -> Result<Vec<EventRow>> {
        let actor = filter.actor_like.as_deref();
        let text = listing_text(fleet.is_some(), cursor.is_some(), actor.is_some());
        let query = sqlx::query(text).bind(workspace.as_str());
        let query = match fleet {
            Some(fleet) => query.bind(fleet),
            None => query,
        };
        let query = match cursor {
            Some(at) => query.bind(at.created_at).bind(at.event_id.as_str()),
            None => query,
        };
        let query = match actor {
            Some(actor) => query.bind(actor),
            None => query,
        };
        let mut connection = self.database.acquire().await?;
        let rows = query
            .bind(filter.since.map_or(NO_LOWER_BOUND, UnixMillis::as_millis))
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
