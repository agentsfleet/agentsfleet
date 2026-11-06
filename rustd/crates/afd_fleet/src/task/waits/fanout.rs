//! Resumable evidence delivery examines one scoped wait per transaction, never a task lock.

use crate::{
    Result,
    error::query,
    task::{Tasks, transaction},
};
use std::time::Duration;

#[derive(sqlx::FromRow)]
struct Progress {
    finished: bool,
    signalled: i64,
}

pub(super) async fn maintain(tasks: &Tasks) -> Result<()> {
    let mut connection = tasks.leases.pool().acquire().await?;
    let rows: Vec<String> = sqlx::query_scalar(include_str!("sql/fanout_due.sql"))
        .bind(PASS_LIMIT)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("select bounded completion fanout cursors"))?;
    drop(connection);
    let mut touched = 0;
    for completion in rows {
        loop {
            if touched >= PASS_LIMIT {
                return Ok(());
            }
            touched += 1;
            match one(tasks, &completion).await {
                Ok(Some(progress)) if !progress.finished => {
                    tracing::debug!(
                        event = "dt_completion_signalled",
                        matched_waits = progress.signalled
                    );
                }
                Ok(_) => break,
                Err(fault) => {
                    tracing::warn!(event = "dt_completion_fanout_deferred", error_code = %fault.code());
                    defer(tasks, &completion).await?;
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn one(tasks: &Tasks, completion: &str) -> Result<Option<Progress>> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    // No SKIP LOCKED on the wait: advancing past a busy row would lose its signal.
    // One wait lock prevents cycles with mixed-input transitions over several tasks.
    // Advance over non-matching workflows too: the pass bounds examined rows,
    // rather than scanning an arbitrary number just to find one match.
    // Every native wait is UUIDv7, above UUID's lower bound on the first page.
    let progress = sqlx::query_as(include_str!("sql/fanout_one.sql"))
        .bind(completion)
        .bind("00000000-0000-0000-0000-000000000000")
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(
            "deliver one completion signal and advance its durable cursor",
        ))?;
    tx.commit()
        .await
        .map_err(query("commit completion fanout progress"))?;
    transaction::recycle(connection);
    Ok(progress)
}

async fn defer(tasks: &Tasks, completion: &str) -> Result<()> {
    let mut connection = tasks.leases.pool().acquire().await?;
    sqlx::query(include_str!("sql/defer_fanout.sql"))
        .bind(completion)
        .bind(Duration::from_secs(30))
        .execute(&mut *connection)
        .await
        .map_err(query(
            "defer a blocked completion cursor without starving later evidence",
        ))?;
    Ok(())
}

const PASS_LIMIT: i64 = 64;
