//! Current work uses the unfinished-task index and excludes private evidence.

use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;

use super::{TaskAfter, TaskPage, TaskScope, TaskSummary, Tasks, fetch, row};
use crate::error::Result;

/// Native ceiling also protects non-HTTP callers from an unbounded list.
const PAGE_MAX: u32 = 100;

#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid7,
    title: String,
    state: String,
    resource_version: i64,
    generation: i64,
    accepted_revision_id: Uuid7,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: jiff::Timestamp,
}

pub(super) async fn read(
    tasks: &Tasks,
    scope: &TaskScope,
    after: Option<&TaskAfter>,
    limit: u32,
) -> Result<TaskPage> {
    let limit = limit.clamp(1, PAGE_MAX);
    let statement = sqlx::query_as(include_str!("sql/list.sql"))
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .bind(after.map(|boundary| boundary.created_at.to_sqlx()))
        .bind(after.map(|boundary| &boundary.id))
        .bind(i64::from(limit) + 1);
    let mut rows: Vec<SummaryRow> = fetch::rows(
        tasks.leases.pool(),
        statement,
        Some(scope),
        "list current durable tasks",
    )
    .await?;
    // A `u32` always fits the `usize` of every target this daemon builds for.
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > limit;
    rows.truncate(limit);
    let items = rows
        .into_iter()
        .map(|stored| {
            Ok(TaskSummary {
                id: stored.id.into(),
                title: stored.title,
                state: row::state(&stored.state)?,
                resource_version: row::counter(stored.resource_version, "resource_version")?,
                generation: row::counter(stored.generation, "generation")?,
                accepted_revision_id: stored.accepted_revision_id,
                created_at: stored.created_at,
                expires_at: stored.expires_at,
            })
        })
        .collect::<Result<_>>()?;
    Ok(TaskPage { items, more })
}
