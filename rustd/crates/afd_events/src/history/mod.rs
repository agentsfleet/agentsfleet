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
mod filter;
mod row;

use afd_core::id::Uuid7;
use afd_db::Db;

use crate::error::{self, Result};

pub use self::cursor::Cursor;
pub use self::filter::{Filter, glob_to_like, parse_since};
pub use self::row::EventRow;

/// What each read was doing, for the operator's log line.
const CONTEXT_FLEET_PAGE: &str = "read a fleet's history";
const CONTEXT_WORKSPACE_PAGE: &str = "read a workspace's history";
const CONTEXT_DETAIL: &str = "read one event";

/// The page a caller gets when they name no size.
pub const DEFAULT_LIMIT: i64 = 50;

/// The largest page this surface will build.
///
/// `LIMIT_MAX`, mirrored. The ceiling is the correlated cost subselect's bound
/// as much as the payload's: it executes once per returned row.
pub const MAX_LIMIT: i64 = 200;

/// The column list both statements are built from, as a macro so `concat!`
/// can splice it at compile time.
///
/// A `const` cannot be concatenated into another `const`, and the alternative —
/// writing the fourteen columns and the cost subselect out twice — is exactly
/// the drift this exists to prevent. `fleet_events_store.zig` repeats its own
/// `EVENTS_SELECT` across eight concatenated variants for want of this.
///
/// `cost_nanos` is a correlated subselect rather than a `LEFT JOIN`: billing
/// writes up to two ledger rows per event — `receive` and `stage`, unique on
/// `(event_id, charge_type)` — so a join would duplicate the event row per leg
/// and a page of 50 would render as 100. The subselect keeps one row per event
/// and yields SQL NULL where no telemetry exists.
macro_rules! select_columns {
    () => {
        "\
SELECT fleet_id::text, event_id, workspace_id::text, actor, event_type,
       status, tokens, wall_ms,
       failure_label, failure_detail, checkpoint_id, resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(te.credit_deducted_nanos)::bigint
          FROM billing.usage_ledger te
         WHERE te.event_id = core.fleet_events.event_id
           AND te.fleet_id = core.fleet_events.fleet_id) AS cost_nanos
FROM core.fleet_events
"
    };
}

/// The listing statement, shared by both entry points.
///
/// `$1` workspace, `$2` fleet or NULL, `$3` cursor timestamp or NULL,
/// `$4` cursor event id, `$5` actor LIKE or NULL, `$6` since or NULL,
/// `$7` limit.
const SELECT_PAGE: &str = concat!(
    select_columns!(),
    "WHERE workspace_id = $1::uuid
  AND ($2::text IS NULL OR fleet_id = $2::uuid)
  AND ($3::bigint IS NULL OR (created_at, event_id) < ($3, $4))
  AND ($5::text IS NULL OR actor LIKE $5)
  AND ($6::bigint IS NULL OR created_at >= $6)
ORDER BY created_at DESC, event_id DESC
LIMIT $7"
);

/// One event by its identifier, scoped to the workspace and fleet that own it.
///
/// The scoping is in the STATEMENT rather than checked after the read: a row
/// belonging to another workspace must not come back and then be filtered, or
/// the filter becomes the only thing standing between two tenants.
///
/// `$1` workspace, `$2` fleet, `$3` event.
const SELECT_ONE: &str = concat!(
    select_columns!(),
    "WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid AND event_id = $3"
);

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

    /// One event, or nothing.
    ///
    /// `Result<Option<_>>`: a row that is not there is an ANSWER, and a
    /// datastore that would not say is a failure. Collapsing the two would make
    /// an outage look like a deleted event to every caller.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn detail(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        event_id: &str,
    ) -> Result<Option<EventRow>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(SELECT_ONE)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(event_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_DETAIL))?;
        found.as_ref().map(EventRow::read).transpose()
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

    #[test]
    fn both_statements_share_one_column_list() {
        // The macro is the single source; this pins that both statements
        // actually expand from it rather than carrying a hand-copied prefix.
        assert!(SELECT_PAGE.starts_with(select_columns!()));
        assert!(SELECT_ONE.starts_with(select_columns!()));
    }
}
