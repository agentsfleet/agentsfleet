//! Task reads use explicit workspace/fleet ownership, including native callers.

use af_core::id::Uuid7;
use sqlx::{PgConnection, Postgres, postgres::PgArguments, query::QueryAs};

use super::{
    DurableTaskRefusal, DurableTaskScope, DurableTaskSnapshot, DurableTasks, fetch,
    row::DurableTaskRow, sql,
};
use crate::error::{Result, query};

pub(super) async fn read(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
) -> Result<DurableTaskSnapshot> {
    fetch::rows(
        tasks.runs.pool(),
        statement(scope, durable_task_id),
        None,
        "read durable task",
    )
    .await?
    .into_iter()
    .next()
    .ok_or(DurableTaskRefusal::NotFound)?
    .snapshot()
}

pub(super) async fn on(
    connection: &mut PgConnection,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
) -> Result<DurableTaskSnapshot> {
    statement(scope, durable_task_id)
        .fetch_optional(connection)
        .await
        .map_err(query("read durable task"))?
        .ok_or(DurableTaskRefusal::NotFound)?
        .snapshot()
}

fn statement<'a>(
    scope: &'a DurableTaskScope,
    durable_task_id: &'a Uuid7,
) -> QueryAs<'a, Postgres, DurableTaskRow, PgArguments> {
    sqlx::query_as(sql::READ)
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
}
