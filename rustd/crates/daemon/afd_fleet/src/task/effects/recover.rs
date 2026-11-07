//! Prepared work is abandoned only after fencing its original possible issuer.

use super::row::Effect;
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, guard, transaction},
};

pub(super) async fn abandon(tasks: &Tasks, lease: TaskLease<'_>, effect: &Effect) -> Result<bool> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    if current.task_id.as_deref() != Some(effect.task_id.as_str()) {
        return Err(TaskRefusal::NotFound.into());
    }
    let _held: Option<String> = sqlx::query_scalar(super::super::sql::LOCK_TASK)
        .bind(&effect.task_id)
        .bind(&effect.fleet_id)
        .bind(&effect.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock abandoned preparation owner"))?;
    let changed = sqlx::query(include_str!("sql/abandon.sql"))
        .bind(&effect.id)
        .bind("refused")
        .bind("prepared")
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(afd_wire::task::TaskState::Running.as_str())
        .execute(&mut *tx)
        .await
        .map_err(query("fence and refuse abandoned unpublished effect"))?
        .rows_affected()
        == 1;
    tx.commit()
        .await
        .map_err(query("commit abandoned effect recovery"))?;
    transaction::recycle(connection);
    Ok(changed)
}
