//! Task reads use explicit workspace/fleet ownership, including native callers.

use af_core::id::Uuid7;
use sqlx::{PgConnection, Postgres, postgres::PgArguments, query::QueryAs};

use super::{TaskRefusal, TaskScope, TaskSnapshot, Tasks, fetch, row::TaskRow, sql};
use crate::error::{Result, query};

pub(super) async fn read(
    tasks: &Tasks,
    scope: &TaskScope,
    task_id: &Uuid7,
) -> Result<TaskSnapshot> {
    fetch::rows(
        tasks.leases.pool(),
        statement(scope, task_id),
        None,
        "read durable task",
    )
    .await?
    .into_iter()
    .next()
    .ok_or(TaskRefusal::NotFound)?
    .snapshot()
}

pub(super) async fn on(
    connection: &mut PgConnection,
    scope: &TaskScope,
    task_id: &Uuid7,
) -> Result<TaskSnapshot> {
    statement(scope, task_id)
        .fetch_optional(connection)
        .await
        .map_err(query("read durable task"))?
        .ok_or(TaskRefusal::NotFound)?
        .snapshot()
}

fn statement<'a>(
    scope: &'a TaskScope,
    task_id: &'a Uuid7,
) -> QueryAs<'a, Postgres, TaskRow, PgArguments> {
    sqlx::query_as(sql::READ)
        .bind(task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
}
