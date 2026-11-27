//! The shared workspace storage lock and encrypted immutable chunks, with event-owned references.

use af_core::id::Uuid7;
use af_wire::{
    durable_task::{CheckpointPlaintext, CheckpointTransferPurpose},
    run::{TurnCheckpointRef, TurnCheckpointResponse},
};
use jiff_sqlx::ToSqlx as _;
use sqlx::types::Json;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun, files::FileOwner, guard, transaction,
};
use crate::{
    Result,
    durable_task::{
        checkpoint::{self, StoredObject, lock_storage},
        storage::{
            WorkspaceKey, recorded,
            registration::{self, NewObject, Registration},
        },
    },
    error::query,
    run::Runs,
};

const FILE: &str = "file";

pub(super) async fn register(
    store: &TurnCheckpoints,
    runs: &Runs,
    presented: TurnRun<'_>,
    checkpoint: &TurnCheckpointRef,
    objects: &[CheckpointPlaintext],
) -> Result<TurnCheckpointResponse> {
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
    storage: &crate::durable_task::storage::R2CheckpointStore,
    objects: &[CheckpointPlaintext],
) -> Result<Vec<af_wire::durable_task::CheckpointObject>> {
    let format = i32::try_from(custody.plan.format_version)
        .map_err(|_overflow| TurnCheckpointRefusal::Incompatible)?;
    let workspace = custody.owner.current.workspace_id.clone();
    lock_storage(&mut custody.owner.tx, &workspace).await?;
    let key = storage
        .keys()
        .for_save(&mut custody.owner.tx, &workspace)
        .await?;
    let content: Vec<_> = objects
        .iter()
        .map(|object| key.content(object, format))
        .collect();
    if content.iter().collect::<BTreeSet<_>>().len() != content.len()
        || objects.iter().any(|object| object.kind != FILE)
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    checkpoint::supersede(
        &mut custody.owner.tx,
        (&workspace, &custody.namespace),
        &content,
        (
            &custody.plan.id,
            &custody.owner.current.id,
            custody.owner.current.fence,
        ),
    )
    .await?;
    let existing = matching(custody, &content).await?;
    let scope = Registration {
        namespace: &custody.namespace,
        format,
        cipher: recorded(&custody.plan.cipher)?,
    };
    let new = missing(runs, storage, scope, (objects, &content), &existing)?;
    reserve(custody, &new, storage.bucket_reference()).await?;
    let registered = matching(custody, &content).await?;
    protect_registered(custody, &registered).await?;
    ordered(custody, storage, registered, &content, &key)
}

fn missing(
    runs: &Runs,
    storage: &crate::durable_task::storage::R2CheckpointStore,
    scope: Registration<'_>,
    (objects, content): (&[CheckpointPlaintext], &[String]),
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
        .map(|(object, content)| {
            registration::prepare(runs, storage.keys(), scope, object, content)
        })
        .collect()
}

fn ordered(
    custody: &FileOwner<'_>,
    storage: &crate::durable_task::storage::R2CheckpointStore,
    registered: Vec<StoredObject>,
    content: &[String],
    key: &WorkspaceKey,
) -> Result<Vec<af_wire::durable_task::CheckpointObject>> {
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
                    &custody.namespace,
                    (
                        &custody.plan.id,
                        &custody.owner.current.id,
                        custody.owner.current.fence,
                    ),
                    key,
                )
        })
        .collect::<Result<_>>()?;
    Ok(items)
}

async fn matching(custody: &mut FileOwner<'_>, content: &[String]) -> Result<Vec<StoredObject>> {
    sqlx::query_as(include_str!(
        "../../durable_task/checkpoint/sql/objects_content.sql"
    ))
    .bind(&custody.owner.current.workspace_id)
    .bind(&custody.namespace)
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
    if !new.is_empty() {
        sqlx::query(checkpoint::OBJECTS_INSERT)
            .bind(Json(new))
            .bind(&custody.owner.current.workspace_id)
            .bind(&custody.plan.id)
            .bind(&custody.owner.current.id)
            .bind(custody.owner.current.fence)
            .bind(bucket)
            .bind(custody.owner.limits.orphan_seconds())
            .execute(&mut *custody.owner.tx)
            .await
            .map_err(query("allocate encrypted turn chunks"))?;
        sqlx::query(checkpoint::PLAN_BYTES)
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
        "../../durable_task/checkpoint/sql/objects_protect.sql"
    ))
    .bind(ids)
    .bind(custody.owner.limits.orphan_seconds().saturating_add(60))
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
