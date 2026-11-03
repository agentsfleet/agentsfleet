//! Task reads use explicit workspace/fleet ownership, including native callers.

use afd_core::id::Uuid7;
use sqlx::PgConnection;

use super::{TaskRefusal, TaskScope, TaskSnapshot, Tasks, row::TaskRow, sql};
use crate::error::{Result, query};

pub(super) async fn read(
    tasks: &Tasks,
    scope: &TaskScope,
    task_id: &Uuid7,
) -> Result<TaskSnapshot> {
    let mut connection = tasks.leases.pool().acquire().await?;
    on(&mut connection, scope, task_id).await
}

pub(super) async fn on(
    connection: &mut PgConnection,
    scope: &TaskScope,
    task_id: &Uuid7,
) -> Result<TaskSnapshot> {
    sqlx::query_as::<_, TaskRow>(sql::READ)
        .bind(task_id.as_str())
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query("read durable task"))?
        .ok_or(TaskRefusal::NotFound)?
        .snapshot()
}
