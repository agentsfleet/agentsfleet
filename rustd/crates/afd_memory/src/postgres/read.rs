//! The Postgres store's reads: the hydration input, recall, the operator page,
//! and a flip's export.

use afd_core::id::Uuid7;
use afd_wire::memory::MAX_ENTRIES_PER_FLEET;
use sqlx::PgConnection;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;

use super::{Failure, PgStore, page, record, sql};
use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Owner, Record};

/// Statement names, for the context a query failure carries.
const CONTEXT_LIST: &str = "memory list";
const CONTEXT_RECALL: &str = "memory recall";
const CONTEXT_EXPORT: &str = "memory export";

/// One prepared statement over the shared seven columns.
type Statement<'q> = Query<'q, sqlx::Postgres, PgArguments>;

/// The most shared rows one hydrate reads before the window spends its budget:
/// one fleet's cap, so a workspace of many publishers costs one fleet's read.
fn shared_scan_limit() -> i64 {
    i64::try_from(MAX_ENTRIES_PER_FLEET).unwrap_or(i64::MAX)
}

/// Runs `statement` and reads every row it returns.
async fn fetch(
    connection: &mut PgConnection,
    statement: Statement<'_>,
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

/// The fleet's own entries, then — for a granted reader — other fleets'
/// shared ones, each newest first.
pub(super) async fn window(store: &PgStore, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>> {
    let failure = Failure::Runner(CONTEXT_LIST);
    store
        .as_memory_role(failure, async |connection| {
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

/// Entries holding `query`, key matches first: the fleet's own, then other
/// fleets' shared ones for a granted reader.
pub(super) async fn search(
    store: &PgStore,
    owner: Owner<'_>,
    reads: bool,
    query: &str,
    limit: usize,
) -> Result<Vec<Record>> {
    let failure = Failure::Runner(CONTEXT_RECALL);
    let pattern = page::pattern(query);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    store
        .as_memory_role(failure, async |connection| {
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

/// One operator page under `view`.
pub(super) async fn page(
    store: &PgStore,
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
    store
        .as_memory_role(failure, async |connection| {
            let mut statement = sqlx::query(page::statement(view, after.is_some()))
                .bind(owner.fleet.as_str())
                .bind(shared);
            if let Some(value) = filter.as_deref() {
                statement = statement.bind(value);
            }
            if let Some(boundary) = after {
                statement = statement
                    .bind(boundary.created_at_ms)
                    .bind(boundary.key)
                    .bind(boundary.fleet.as_str());
            }
            fetch(connection, statement.bind(limit), failure).await
        })
        .await
}

/// Every entry in `workspace`.
pub(super) async fn export(store: &PgStore, workspace: &Uuid7) -> Result<Vec<Record>> {
    let failure = Failure::Runner(CONTEXT_EXPORT);
    store
        .as_memory_role(failure, async |connection| {
            let all = sqlx::query(sql::SELECT_WORKSPACE_ENTRIES).bind(workspace.as_str());
            fetch(connection, all, failure).await
        })
        .await
}
