//! Shared workspace quota and encrypted immutable chunks, with event-owned references.

use af_core::id::Uuid7;
use af_wire::{
    run::{TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointResponse},
    task::CheckpointTransferPurpose,
};
use jiff_sqlx::ToSqlx as _;
use sqlx::types::Json;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun, files::FileOwner, guard, transaction,
};
use crate::{
    Result,
    error::query,
    run::Runs,
    task::{
        checkpoint::{self, StoredObject, content_identity, lock_quota},
        storage::registration::{self, NewObject, Registration},
    },
};

const WORKSPACE_BYTES: i64 = 50 * 1024 * 1024 * 1024;
const ORPHAN_SECONDS: i32 = 60 * 60;
const FILE: &str = "file";

pub(super) async fn register(
    store: &TurnCheckpoints,
    runs: &Runs,
    presented: TurnRun<'_>,
    request: &TurnCheckpointCommand,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::Objects {
        checkpoint,
        objects,
    } = &request.action
    else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let storage = store.files()?;
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(
        runs,
        &mut connection,
        presented,
        checkpoint,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    let items = register_content(&mut custody, runs, storage, objects).await?;
    custody.commit().await?;
    transaction::recycle(connection);
    Ok(TurnCheckpointResponse::Objects(items))
}

async fn register_content(
    custody: &mut FileOwner<'_>,
    runs: &Runs,
    storage: &crate::task::storage::R2CheckpointStore,
    objects: &[af_wire::task::CheckpointPlaintext],
) -> Result<Vec<af_wire::task::CheckpointObject>> {
    let format = i32::try_from(custody.plan.format_version)
        .map_err(|_overflow| TurnCheckpointRefusal::Incompatible)?;
    let workspace = custody.owner.current.workspace_id.clone();
    lock_quota(&mut custody.owner.tx, &workspace).await?;
    let identity = content_identity(
        runs,
        storage,
        &mut custody.owner.tx,
        &workspace,
        &custody.namespace,
    )
    .await?;
    let content: Vec<_> = objects
        .iter()
        .map(|object| identity.content(object, format))
        .collect();
    if content.iter().collect::<BTreeSet<_>>().len() != content.len()
        || objects.iter().any(|object| object.kind != FILE)
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let existing = matching(custody, &identity.id, &content).await?;
    let new = missing(
        runs,
        storage,
        Registration {
            workspace: &workspace,
            namespace: &custody.namespace,
            identity: &identity.id,
            format,
        },
        objects,
        &content,
        &existing,
    )?;
    reserve(custody, &new, storage.bucket_reference()).await?;
    let registered = matching(custody, &identity.id, &content).await?;
    protect_registered(custody, &registered).await?;
    ordered(custody, storage, registered, &content)
}

fn missing(
    runs: &Runs,
    storage: &crate::task::storage::R2CheckpointStore,
    scope: Registration<'_>,
    objects: &[af_wire::task::CheckpointPlaintext],
    content: &[String],
    existing: &[StoredObject],
) -> Result<Vec<NewObject>> {
    let found: BTreeSet<_> = existing
        .iter()
        .map(|object| object.content_identity.as_str())
        .collect();
    objects
        .iter()
        .zip(content)
        .filter(|(_, content)| !found.contains(content.as_str()))
        .map(|(object, content)| registration::prepare(runs, storage, scope, object, content))
        .collect()
}

fn ordered(
    custody: &FileOwner<'_>,
    storage: &crate::task::storage::R2CheckpointStore,
    registered: Vec<StoredObject>,
    content: &[String],
) -> Result<Vec<af_wire::task::CheckpointObject>> {
    let mut by_content: BTreeMap<_, _> = registered
        .into_iter()
        .map(|object| (object.content_identity.clone(), object))
        .collect();
    let items = content
        .iter()
        .map(|content| {
            by_content
                .remove(content)
                .ok_or(TurnCheckpointRefusal::Incompatible)?
                .wire_scoped(
                    storage,
                    &custody.owner.current.workspace_id,
                    &custody.namespace,
                    (
                        &custody.plan.id,
                        &custody.owner.current.id,
                        custody.owner.current.fence,
                    ),
                )
        })
        .collect::<Result<_>>()?;
    Ok(items)
}

async fn matching(
    custody: &mut FileOwner<'_>,
    identity: &Uuid7,
    content: &[String],
) -> Result<Vec<StoredObject>> {
    sqlx::query_as(include_str!(
        "../../task/checkpoint/sql/objects_content.sql"
    ))
    .bind(&custody.owner.current.workspace_id)
    .bind(identity)
    .bind(content)
    .fetch_all(&mut *custody.owner.tx)
    .await
    .map_err(query("hold matching event-owned content"))
}

async fn reserve(custody: &mut FileOwner<'_>, new: &[NewObject], bucket: &str) -> Result<()> {
    let bytes = new
        .iter()
        .try_fold(0_i64, |sum, object| sum.checked_add(object.accounted_bytes))
        .ok_or(TurnCheckpointRefusal::Limit)?;
    if custody
        .plan
        .uploaded_bytes
        .checked_add(bytes)
        .is_none_or(|sum| sum > custody.plan.upload_budget_bytes)
    {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    let reserved = sqlx::query(checkpoint::QUOTA_RESERVE)
        .bind(&custody.owner.current.workspace_id)
        .bind(bytes)
        .bind(WORKSPACE_BYTES)
        .execute(&mut *custody.owner.tx)
        .await
        .map_err(query("reserve shared turn storage quota"))?;
    if reserved.rows_affected() != 1 {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    if !new.is_empty() {
        sqlx::query(checkpoint::OBJECTS_INSERT)
            .bind(Json(new))
            .bind(&custody.owner.current.workspace_id)
            .bind(&custody.plan.id)
            .bind(&custody.owner.current.id)
            .bind(custody.owner.current.fence)
            .bind(bucket)
            .bind(ORPHAN_SECONDS)
            .execute(&mut *custody.owner.tx)
            .await
            .map_err(query("allocate encrypted turn chunks"))?;
        sqlx::query(include_str!("sql/plan_bytes.sql"))
            .bind(&custody.plan.id)
            .bind(bytes)
            .execute(&mut *custody.owner.tx)
            .await
            .map_err(query("account turn upload budget"))?;
    }
    Ok(())
}

pub(super) async fn attach(custody: &mut FileOwner<'_>, ids: &[Uuid7]) -> Result<()> {
    sqlx::query(include_str!("sql/refs_insert.sql"))
        .bind(&custody.plan.id)
        .bind(&custody.owner.current.workspace_id)
        .bind(ids)
        .execute(&mut *custody.owner.tx)
        .await
        .map_err(query("protect scoped turn objects"))?;
    Ok(())
}

pub(super) async fn scoped(
    custody: &mut FileOwner<'_>,
    ids: &[Uuid7],
) -> Result<Vec<StoredObject>> {
    scoped_held(&mut custody.owner, &custody.plan, &custody.namespace, ids).await
}

pub(super) async fn scoped_held(
    owner: &mut guard::TurnRunGuard<'_>,
    plan: &super::row::Plan,
    namespace: &str,
    ids: &[Uuid7],
) -> Result<Vec<StoredObject>> {
    let objects: Vec<StoredObject> = sqlx::query_as(include_str!("sql/objects_scope.sql"))
        .bind(ids)
        .bind(&owner.current.workspace_id)
        .bind(&plan.id)
        .bind(namespace)
        .fetch_all(&mut *owner.tx)
        .await
        .map_err(query("hold exact event-owned object scope"))?;
    if objects.len() != ids.len() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(objects)
}

pub(super) async fn protect(custody: &mut FileOwner<'_>, ids: &[Uuid7]) -> Result<()> {
    sqlx::query(include_str!(
        "../../task/checkpoint/sql/objects_protect.sql"
    ))
    .bind(ids)
    .bind(ORPHAN_SECONDS + 60)
    .bind(custody.pin.expires_at.to_sqlx())
    .execute(&mut *custody.owner.tx)
    .await
    .map_err(query("retain turn objects beyond transfer custody"))?;
    custody.fresh().await
}

pub(super) fn ids(ids: &[af_wire::run::CheckpointOperationId]) -> Result<Vec<Uuid7>> {
    let values = ids.iter().map(guard::id).collect::<Result<Vec<_>>>()?;
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(values)
}

async fn protect_registered(
    custody: &mut FileOwner<'_>,
    registered: &[StoredObject],
) -> Result<()> {
    let eligible: Vec<_> = registered
        .iter()
        .filter(|object| {
            object.verified
                || object.belongs(
                    &custody.plan.id,
                    &custody.owner.current.id,
                    custody.owner.current.fence,
                )
        })
        .map(|object| object.id.clone())
        .collect();
    attach(custody, &eligible).await?;
    protect(custody, &eligible).await?;
    Ok(())
}
