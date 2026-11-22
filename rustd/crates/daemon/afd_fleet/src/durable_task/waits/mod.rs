//! Durable dependency ownership; every wake still uses ordinary turns. The
//! wait lives on its task's row (`fleet.durable_tasks.wait_*`).

use super::github::{BoundPr, status::Snapshot};
use crate::{Result, error::query};
use af_core::id::Uuid7;
use sqlx::{PgConnection, types::Json};

pub(in crate::durable_task) async fn arm(
    connection: &mut PgConnection,
    task: &Uuid7,
    run: &Uuid7,
    target: &BoundPr,
    dependency: &Snapshot,
) -> Result<()> {
    let generation: i64 = sqlx::query_scalar(include_str!("sql/arm.sql"))
        .bind(task)
        .bind(run)
        .bind(&target.head)
        .bind(Json(&dependency.checks))
        .fetch_one(connection)
        .await
        .map_err(query("arm exact-head wait before releasing execution"))?;
    tracing::info!(
        event = "dt_wait_armed",
        durable_task_id = %task,
        wait_sequence = generation
    );
    Ok(())
}

pub(in crate::durable_task) async fn cancel(
    connection: &mut PgConnection,
    task: &Uuid7,
) -> Result<()> {
    sqlx::query(include_str!("sql/cancel.sql"))
        .bind(task)
        .execute(connection)
        .await
        .map_err(query(
            "invalidate previous task wait in the owning transition",
        ))?;
    Ok(())
}
