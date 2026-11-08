//! Owner retention precedes orphan collection; remote deletion keeps durable custody.

mod owners;
mod purge;

use crate::{
    Result,
    error::query,
    task::{Tasks, transaction},
};
use sqlx::PgConnection;
use std::collections::BTreeMap;

/// One bounded pass; the daemon supervises cancellation of the owning future.
pub(in crate::task) async fn maintain(
    tasks: &Tasks,
    cursor: &mut crate::task::CheckpointMaintenanceCursor,
) -> Result<crate::task::CheckpointMaintenance> {
    let Some(storage) = tasks.dependencies.storage.as_ref() else {
        return Ok(crate::task::CheckpointMaintenance::default());
    };
    let mut result = crate::task::CheckpointMaintenance::default();
    let (owners, deferred) = owners::retain(tasks, cursor).await?;
    result.owners = owners;
    result.deferred = deferred;
    result.bases = owners::bases(tasks).await?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let candidates: Vec<Candidate> = sqlx::query_as(include_str!("../sql/orphans.sql"))
        .bind(super::ORPHAN_SECONDS)
        .bind(super::GC_BATCH)
        .fetch_all(&mut **connection)
        .await
        .map_err(query("page genuinely unreferenced private objects"))?;
    transaction::recycle(connection);
    let mut workspaces: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for object in candidates {
        workspaces
            .entry(object.workspace_id)
            .or_default()
            .push(object.id);
    }
    for (workspace, ids) in workspaces {
        let mut connection = transaction::acquire(tasks.leases.pool()).await?;
        let mut tx = transaction::begin(&mut connection).await?;
        if lock(&mut tx, &workspace).await? {
            // A second statement observes refs committed while the first waited for object locks.
            let _locked: Vec<String> = sqlx::query_scalar(include_str!("../sql/orphan_lock.sql"))
                .bind(&workspace)
                .bind(&ids)
                .fetch_all(&mut *tx)
                .await
                .map_err(query("serialize object tombstones against new references"))?;
            let tombstoned: Vec<String> =
                sqlx::query_scalar(include_str!("../sql/orphan_tombstone.sql"))
                    .bind(&workspace)
                    .bind(&ids)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(query("tombstone only the freshly rechecked orphan set"))?;
            result.objects += sqlx::query(include_str!("../sql/orphan_delete.sql"))
                .bind(&workspace)
                .bind(&tombstoned)
                .execute(&mut *tx)
                .await
                .map_err(query(
                    "erase wrapped keys while retaining opaque purge custody",
                ))?
                .rows_affected();
        }
        tx.commit()
            .await
            .map_err(query("commit orphan custody transfer to durable purges"))?;
        transaction::recycle(connection);
    }
    let (purged, deferred) = purge::run(tasks, storage).await?;
    result.purged = purged;
    result.deferred += deferred;
    Ok(result)
}

#[derive(sqlx::FromRow)]
struct Candidate {
    id: String,
    workspace_id: String,
}

async fn lock(connection: &mut PgConnection, workspace: &str) -> Result<bool> {
    let parent: Option<String> = sqlx::query_scalar(crate::task::sql::LOCK_WORKSPACE)
        .bind(workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("hold private storage workspace"))?;
    if parent.is_none() {
        return Ok(false);
    }
    super::identity::quota(connection, workspace).await?;
    Ok(true)
}
