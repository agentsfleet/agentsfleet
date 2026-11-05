//! Indexed expiration needs no caller or agent to enforce the task wall limit.

use crate::{
    Result,
    error::query,
    task::{TaskScope, Tasks, guard, transaction},
};
use afd_wire::task::TaskState;
use std::time::Duration;

#[derive(sqlx::FromRow)]
struct Expired {
    task_id: String,
    fleet_id: String,
    workspace_id: String,
}

pub(super) async fn maintain(tasks: &Tasks) -> Result<()> {
    let mut connection = tasks.leases.pool().acquire().await?;
    sqlx::query(include_str!("sql/collect_completions.sql"))
        .bind(COMPLETION_RETENTION)
        .bind(256_i64)
        .execute(&mut *connection)
        .await
        .map_err(query("collect bounded verified-completion inbox rows"))?;
    let expired: Vec<Expired> = sqlx::query_as(include_str!("sql/expired_tasks.sql"))
        .bind(64_i64)
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("select indexed task wall deadlines"))?;
    drop(connection);
    for owner in expired {
        if let Err(fault) = expire(tasks, &owner).await {
            tracing::warn!(event = "dt_expiration_deferred", task_id = owner.task_id.as_str(), error_code = %fault.code());
        }
    }
    Ok(())
}

async fn expire(tasks: &Tasks, owner: &Expired) -> Result<()> {
    let scope = TaskScope {
        fleet_id: guard::id(&owner.fleet_id, "fleet_id")?,
        workspace_id: guard::id(&owner.workspace_id, "workspace_id")?,
    };
    let task = guard::id(&owner.task_id, "task_id")?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    guard::scope(&mut tx, &scope).await?;
    let _held: Option<String> = sqlx::query_scalar(crate::task::sql::LOCK_TASK)
        .bind(&owner.task_id)
        .bind(&owner.fleet_id)
        .bind(&owner.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock task for native wall expiration"))?;
    let reason: Option<String> = sqlx::query_scalar(include_str!("sql/expire.sql"))
        .bind(&owner.task_id)
        .bind(TaskState::Exhausted.as_str())
        .bind("wall_deadline_reached")
        .bind("lease_limit_reached")
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(
            "expire task against post-lock deadline or spent attempts",
        ))?;
    if reason.is_some() {
        super::cancel(&mut tx, &owner.task_id).await?;
        crate::task::continuation::retire(&mut tx, &owner.task_id).await?;
    }
    tx.commit()
        .await
        .map_err(query("commit native wall expiration"))?;
    transaction::recycle(connection);
    if let Some(reason) = reason {
        if let Ok(snapshot) = crate::task::read::read(tasks, &scope, &task).await {
            tasks
                .publish_changed(&scope, &task, snapshot.resource_version)
                .await;
        }
        tracing::info!(
            event = "dt_transition",
            task_id = owner.task_id.as_str(),
            state = TaskState::Exhausted.as_str(),
            reason
        );
    }
    Ok(())
}

const COMPLETION_RETENTION: Duration = Duration::from_secs(3 * 24 * 60 * 60);
