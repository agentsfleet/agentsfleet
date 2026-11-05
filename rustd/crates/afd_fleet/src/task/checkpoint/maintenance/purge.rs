//! Idempotent remote DELETE and exact-once quota release survive lost replies.

use futures_util::{StreamExt as _, stream};
use crate::{Result, error::query, task::{TaskRefusal, Tasks, guard as task_guard, storage::R2CheckpointStore, transaction}};

const RECHECK_SECONDS: i32 = 5 * 60;

#[derive(sqlx::FromRow)]
struct Purge { object_id: String, workspace_id: String, identity_key_id: String, bucket_reference: String }

pub(super) async fn run(tasks: &Tasks, storage: &R2CheckpointStore) -> Result<(u64, u64)> {
    let session = storage.session().await?;
    let (mut completed, mut deferred, mut attempted) = (0, 0, 0_i64);
    while attempted < super::super::GC_BATCH {
        let purges = claim(tasks, (super::super::GC_BATCH - attempted).min(4)).await?;
        if purges.is_empty() { break; }
        attempted += i64::try_from(purges.len()).map_err(|_overflow| TaskRefusal::LimitExhausted)?;
        let session = &session;
        let mut work = stream::iter(purges.into_iter().map(|purge| async move {
            let result = async {
                if purge.bucket_reference != storage.bucket_reference() { return Err(TaskRefusal::StorageUnavailable.into()); }
                session.delete(&task_guard::id(&purge.workspace_id, "workspace_id")?,
                    &task_guard::id(&purge.identity_key_id, "identity_key_id")?, &task_guard::id(&purge.object_id, "object_id")?).await?;
                acknowledge(tasks, &purge).await
            }.await;
            (purge.object_id, result)
        })).buffer_unordered(4);
        while let Some((id, result)) = work.next().await {
            match result {
                Ok(()) => completed += 1,
                Err(error) => { deferred += 1; tracing::warn!(event = "dt_storage_purge_deferred", object_id = id, error_code = %error.code()); }
            }
        }
    }
    Ok((completed, deferred))
}

async fn claim(tasks: &Tasks, limit: i64) -> Result<Vec<Purge>> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let purges = sqlx::query_as(include_str!("../sql/purge_claim.sql")).bind(limit).bind(RECHECK_SECONDS)
        .fetch_all(&mut *tx).await.map_err(query("claim a worker-sized due storage purge batch"))?;
    tx.commit().await.map_err(query("commit purge claims before remote deletion"))?;
    transaction::recycle(connection);
    Ok(purges)
}

async fn acknowledge(tasks: &Tasks, purge: &Purge) -> Result<()> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    // This order is shared with registration and verification. Missing owners need no recreated quota row.
    let _parent = super::lock(&mut tx, &purge.workspace_id).await?;
    sqlx::query(include_str!("../sql/purge_complete.sql")).bind(&purge.object_id).bind(&purge.workspace_id)
        .execute(&mut *tx).await.map_err(query("release acknowledged remote storage bytes once"))?;
    tx.commit().await.map_err(query("commit remote deletion receipt and exact quota release"))?;
    transaction::recycle(connection);
    Ok(())
}
