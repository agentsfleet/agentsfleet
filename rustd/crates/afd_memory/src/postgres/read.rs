//! The Postgres store's reads: the hydration input, recall, and the operator
//! page.

use afd_db::statement::Bound;
use afd_wire::memory::MAX_ENTRIES_PER_FLEET;
use sqlx::PgConnection;

use super::{Failure, PgStore, page, record};
use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Owner, Record};
use crate::sql;

/// Statement names, for the context a query failure carries.
const CONTEXT_LIST: &str = "memory list";
const CONTEXT_RECALL: &str = "memory recall";

/// The most shared rows one hydrate reads before the window spends its budget:
/// one fleet's cap, so a workspace of many publishers costs one fleet's read.
fn shared_scan_limit() -> i64 {
    i64::try_from(MAX_ENTRIES_PER_FLEET).unwrap_or(i64::MAX)
}

/// Runs `statement` and reads every row it returns.
async fn fetch(
    connection: &mut PgConnection,
    statement: Bound<'_>,
    failure: Failure,
) -> Result<Vec<Record>> {
    statement
        .fetch_all(&mut *connection)
        .await
        .map_err(|source| failure.raise(source))?
        .iter()
        .map(|row| record(row, failure))
        .collect()
}

impl PgStore {
    /// Every entry `owner.fleet` holds, newest first; then, when `reads`, the
    /// workspace-visible entries other fleets in `owner.workspace` hold, newest
    /// first and bounded by one fleet's cap.
    ///
    /// # Errors
    /// The database cannot answer.
    pub async fn window(&self, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>> {
        let failure = Failure::Runner(CONTEXT_LIST);
        self.as_memory_role(failure, async |connection| {
            let own = sqlx::query(sql::SELECT_ALL_FOR_FLEET).bind(owner.fleet.as_str());
            let mut rows = fetch(connection, own, failure).await?;
            if reads {
                let shared = sqlx::query(sql::SELECT_SHARED_IN_WORKSPACE)
                    .bind(owner.workspace.as_str())
                    .bind(owner.fleet.as_str())
                    .bind(shared_scan_limit());
                rows.extend(fetch(connection, shared, failure).await?);
            }
            Ok(rows)
        })
        .await
    }

    /// Entries whose key or content holds `query`, ignoring case: the fleet's
    /// own and, when `reads`, other fleets' shared ones; key matches first, then
    /// newest first, at most `limit` of each.
    ///
    /// # Errors
    /// The database cannot answer.
    pub async fn search(
        &self,
        owner: Owner<'_>,
        reads: bool,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Record>> {
        let failure = Failure::Runner(CONTEXT_RECALL);
        let pattern = page::pattern(query);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.as_memory_role(failure, async |connection| {
            let own = sqlx::query(sql::SEARCH_OWN)
                .bind(owner.fleet.as_str())
                .bind(pattern.as_str())
                .bind(limit);
            let mut rows = fetch(connection, own, failure).await?;
            if reads {
                let shared = sqlx::query(sql::SEARCH_SHARED)
                    .bind(owner.workspace.as_str())
                    .bind(owner.fleet.as_str())
                    .bind(pattern.as_str())
                    .bind(limit);
                rows.extend(fetch(connection, shared, failure).await?);
            }
            Ok(rows)
        })
        .await
    }

    /// One operator page under `view`, newest-created first, holding other
    /// fleets' shared entries too when `reads`.
    ///
    /// # Errors
    /// The database cannot answer.
    pub async fn page(
        &self,
        owner: Owner<'_>,
        reads: bool,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>> {
        let failure = Failure::Operator(view.detail());
        // Built before the pipeline so the pattern outlives every borrow the
        // statement holds.
        let filter = page::filter(view);
        let shared = reads.then_some(owner.workspace.as_str());
        self.as_memory_role(failure, async |connection| {
            let mut statement = sqlx::query(page::statement(view, after.is_some()))
                .bind(owner.fleet.as_str())
                .bind(shared);
            if let Some(value) = filter.as_deref() {
                statement = statement.bind(value);
            }
            if let Some(boundary) = after {
                statement = statement
                    .bind(jiff_sqlx::ToSqlx::to_sqlx(boundary.created_at))
                    .bind(boundary.key)
                    .bind(boundary.fleet.as_str());
            }
            fetch(connection, statement.bind(limit), failure).await
        })
        .await
    }
}
