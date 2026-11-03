//! Existing fleet/lease authority, held through the native task transaction.

use afd_core::id::Uuid7;
use afd_db::Db;
use sqlx::PgConnection;

use super::{TaskLease, TaskRefusal, TaskScope, sql};
use crate::error::{Result, lease_not_found, query, stale_fence};
use crate::lease::{sql::LEASE_STATUS_ACTIVE, standing::fence_holds};

const CONTEXT: &str = "prove durable task lease ownership";

#[derive(Debug, sqlx::FromRow)]
pub(super) struct LeaseRow {
    pub fleet_id: String,
    pub workspace_id: String,
    pub tenant_id: String,
    pub source_admission_id: String,
    pub task_id: Option<String>,
    pub fence: i64,
    pub live_fence: i64,
    pub revision_id: Option<String>,
}

pub(super) async fn lease_scope(database: &Db, presented: TaskLease<'_>) -> Result<TaskScope> {
    let mut connection = database.acquire().await?;
    let (fleet, workspace): (String, String) = sqlx::query_as(sql::LEASE_SCOPE)
        .bind(presented.lease_id.as_str())
        .bind(presented.runner_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(lease_not_found)?;
    Ok(TaskScope {
        fleet_id: id(&fleet, "fleet_id")?,
        workspace_id: id(&workspace, "workspace_id")?,
    })
}

pub(super) async fn lease(
    connection: &mut PgConnection,
    presented: TaskLease<'_>,
) -> Result<LeaseRow> {
    // A short ownership read precedes the lock so fleet deletion cannot wait
    // on our slot while we wait on its parent row's foreign-key lock.
    let scope: Option<(String, String)> = sqlx::query_as(sql::LEASE_SCOPE)
        .bind(presented.lease_id.as_str())
        .bind(presented.runner_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let (fleet, workspace) = scope.ok_or_else(lease_not_found)?;
    lock_scope(connection, &fleet, &workspace).await?;
    sqlx::query_scalar::<_, String>(sql::LOCK_SLOT)
        .bind(presented.lease_id.as_str())
        .bind(presented.runner_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(lease_not_found)?;
    // Slot and parent locks are already held when the existing deadline/fence
    // is checked. A row-lock wait cannot leave a pre-wait deadline accepted.
    let row: LeaseRow = sqlx::query_as(sql::LEASE)
        .bind(presented.lease_id.as_str())
        .bind(presented.runner_id.as_str())
        .bind(LEASE_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(lease_not_found)?;
    if !fence_holds(row.fence, row.live_fence, presented.fencing_token) {
        return Err(stale_fence());
    }
    Ok(row)
}

pub(super) async fn scope(connection: &mut PgConnection, scope: &TaskScope) -> Result<()> {
    lock_scope(
        connection,
        scope.fleet_id.as_str(),
        scope.workspace_id.as_str(),
    )
    .await?;
    let _slot = sqlx::query_scalar::<_, String>(sql::LOCK_SCOPE_SLOT)
        .bind(scope.fleet_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

async fn lock_scope(connection: &mut PgConnection, fleet: &str, workspace: &str) -> Result<()> {
    sqlx::query_scalar::<_, String>(sql::LOCK_SCOPE)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::NotFound)?;
    Ok(())
}

pub(super) fn id(text: &str, column: &'static str) -> Result<Uuid7> {
    Uuid7::parse(text).map_err(crate::error::row_malformed("fleet.tasks", column))
}
