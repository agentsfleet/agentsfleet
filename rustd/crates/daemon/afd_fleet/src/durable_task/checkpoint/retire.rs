//! A workspace's key changed or was lost: everything it sealed leaves together.
//!
//! Deleting a generation clears the head that named it, so its owner starts
//! from its original message. Deleting an object row records its bucket delete
//! in `runner.checkpoint_purges`, which the purge pass acknowledges.

use af_core::id::Uuid7;
use sqlx::{Postgres, Transaction};

use crate::{
    Result,
    durable_task::{
        storage::{CheckpointKeys, RetiredCheckpoints},
        transaction,
    },
    error::query,
};

/// Generations and bases go first, each waiting for an operation that holds
/// its row; the storage lock comes after them, as it does in a registration.
async fn retire_in(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &Uuid7,
) -> Result<RetiredCheckpoints> {
    let generations = sqlx::query(include_str!("sql/retire_checkpoints.sql"))
        .bind(workspace)
        .execute(&mut **tx)
        .await
        .map_err(query("retire a workspace's checkpoint generations"))?
        .rows_affected();
    sqlx::query(include_str!("sql/retire_bases.sql"))
        .bind(workspace)
        .execute(&mut **tx)
        .await
        .map_err(query("retire a workspace's repository bases"))?;
    super::lock_storage(tx, workspace).await?;
    let objects = sqlx::query(include_str!("sql/retire_objects.sql"))
        .bind(workspace)
        .execute(&mut **tx)
        .await
        .map_err(query("owe the bucket delete of every retired object"))?
        .rows_affected();
    Ok(RetiredCheckpoints {
        generations,
        objects,
    })
}

/// Retires what an unusable key sealed, so no restore is offered it again.
pub(crate) async fn retire(database: &afd_db::Db, workspace: &Uuid7) -> Result<RetiredCheckpoints> {
    let mut connection = transaction::acquire(database).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let retired = retire_in(&mut tx, workspace).await?;
    tx.commit()
        .await
        .map_err(query("commit the retirement of unreadable checkpoints"))?;
    transaction::recycle(connection);
    Ok(retired)
}

/// Writes a new key and retires everything the old one sealed, in one
/// transaction: the workspace's next save is a full one. A workspace that does
/// not exist answers nothing.
pub(crate) async fn rotate(
    keys: &CheckpointKeys,
    database: &afd_db::Db,
    workspace: &Uuid7,
) -> Result<Option<RetiredCheckpoints>> {
    let mut connection = transaction::acquire(database).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let known: Option<Uuid7> = sqlx::query_scalar(crate::durable_task::sql::LOCK_WORKSPACE)
        .bind(workspace)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold the workspace whose checkpoint key rotates"))?;
    if known.is_none() {
        return Ok(None);
    }
    let retired = retire_in(&mut tx, workspace).await?;
    keys.replace(&mut tx, workspace).await?;
    tx.commit()
        .await
        .map_err(query("commit a rotated checkpoint key"))?;
    transaction::recycle(connection);
    tracing::info!(
        event = "checkpoint_key_rotated",
        workspace_id = %workspace,
        generations = retired.generations,
        objects = retired.objects
    );
    Ok(Some(retired))
}
