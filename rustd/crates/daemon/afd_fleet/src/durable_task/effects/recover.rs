//! Prepared work is abandoned only after fencing its original possible issuer.

use super::row::Effect;
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTasks, guard, transaction},
    error::query,
};
use af_core::id::Uuid7;

pub(super) async fn abandon(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    effect: &Effect,
) -> Result<bool> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    if current.durable_task_id.as_ref().map(Uuid7::as_str) != Some(effect.durable_task_id.as_str())
    {
        return Err(DurableTaskRefusal::NotFound.into());
    }
    let _held: Option<String> = sqlx::query_scalar(super::super::sql::LOCK_DURABLE_TASK)
        .bind(&effect.durable_task_id)
        .bind(&effect.fleet_id)
        .bind(&effect.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock abandoned preparation owner"))?;
    let changed = sqlx::query(include_str!("sql/abandon.sql"))
        .bind(&effect.id)
        .bind("refused")
        .bind("prepared")
        .bind(crate::run::sql::RUN_STATUS_ACTIVE)
        .bind(af_wire::durable_task::DurableTaskState::Running.as_str())
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
