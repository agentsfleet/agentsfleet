//! One workspace quota lock and bounded set operations arbitrate upload owners.

use super::{guard, identity, object_row::Object, row::Plan};
use crate::task::storage::R2CheckpointStore;
use crate::task::storage::registration::NewObject;
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, guard as task_guard, transaction},
};
use af_core::id::Uuid7;
use af_wire::task::{
    CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPlaintext, CheckpointTransferPurpose,
};
use sqlx::{PgConnection, types::Json};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::task) async fn register(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointObjectsInput>,
) -> Result<CheckpointObjectsPage> {
    let storage = guard::storage(tasks)?;
    let pin = task_guard::id(&input.pin_id, "pin_id")?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (_owned, plan, pin) = guard::owned(
        &mut tx,
        lease,
        &input.task,
        checkpoint,
        &pin,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    let items = registered(tasks, storage, &mut tx, &plan, &pin, input).await?;
    tx.commit()
        .await
        .map_err(query("commit registered object ownership and quota"))?;
    transaction::recycle(connection);
    Ok(CheckpointObjectsPage {
        items,
        next_cursor: None,
    })
}

async fn registered(
    tasks: &Tasks,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    plan: &Plan,
    pin: &super::row::Pin,
    input: &CheckpointObjectsInput,
) -> Result<Vec<af_wire::task::CheckpointObject>> {
    let (identity, content) = allocate(tasks, storage, connection, plan, input).await?;
    let objects = matching(connection, plan, &identity, &content).await?;
    let eligible: Vec<_> = objects
        .iter()
        .filter(|object| object.verified || object.own(plan))
        .map(|object| object.id.clone())
        .collect();
    attach(connection, plan, &eligible).await?;
    super::transfer::protect(connection, &eligible, pin).await?;
    let mut by_content: BTreeMap<_, _> = objects
        .into_iter()
        .map(|object| (object.content_identity.clone(), object))
        .collect();
    content
        .iter()
        .map(|content| {
            by_content
                .remove(content)
                .ok_or(TaskRefusal::CheckpointIncompatible)?
                .wire(storage, plan)
        })
        .collect()
}

async fn allocate(
    tasks: &Tasks,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    plan: &Plan,
    input: &CheckpointObjectsInput,
) -> Result<(Uuid7, Vec<String>)> {
    identity::quota(connection, &plan.workspace_id).await?;
    let identity = identity::load(
        &tasks.leases,
        storage,
        connection,
        &plan.workspace_id,
        &plan.repository,
    )
    .await?;
    let content: Vec<_> = input
        .objects
        .iter()
        .map(|object| identity.content(object, plan.format_version))
        .collect();
    if content.iter().collect::<BTreeSet<_>>().len() != content.len() {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let existing = matching(connection, plan, &identity.id, &content).await?;
    let found: BTreeSet<_> = existing
        .iter()
        .map(|object| object.content_identity.as_str())
        .collect();
    let new = input
        .objects
        .iter()
        .zip(&content)
        .filter(|(_, content)| !found.contains(content.as_str()))
        .map(|(descriptor, content)| {
            prepare(
                tasks,
                storage,
                plan,
                &plan.workspace_id,
                &identity.id,
                descriptor,
                content,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    reserve(connection, plan, &new, storage).await?;
    Ok((identity.id, content))
}

async fn matching(
    connection: &mut PgConnection,
    plan: &Plan,
    identity: &Uuid7,
    content: &[String],
) -> Result<Vec<Object>> {
    sqlx::query_as(include_str!("sql/objects_content.sql"))
        .bind(&plan.workspace_id)
        .bind(identity)
        .bind(content)
        .fetch_all(connection)
        .await
        .map_err(query("lock matching immutable content batch"))
}

async fn reserve(
    connection: &mut PgConnection,
    plan: &Plan,
    new: &[NewObject],
    storage: &R2CheckpointStore,
) -> Result<()> {
    let new_bytes = new
        .iter()
        .try_fold(0_i64, |sum, object| sum.checked_add(object.accounted_bytes))
        .ok_or(TaskRefusal::LimitExhausted)?;
    if plan
        .uploaded_bytes
        .checked_add(new_bytes)
        .is_none_or(|bytes| bytes > plan.upload_budget_bytes)
    {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    let reserved = sqlx::query(include_str!("sql/quota_reserve.sql"))
        .bind(&plan.workspace_id)
        .bind(new_bytes)
        .bind(super::WORKSPACE_BYTES)
        .execute(&mut *connection)
        .await
        .map_err(query("reserve unique private storage bytes"))?;
    if reserved.rows_affected() != 1 {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    if !new.is_empty() {
        sqlx::query(include_str!("sql/objects_insert.sql"))
            .bind(Json(new))
            .bind(&plan.workspace_id)
            .bind(&plan.id)
            .bind(&plan.owner_lease_id)
            .bind(plan.owner_fence)
            .bind(storage.bucket_reference())
            .bind(super::ORPHAN_SECONDS)
            .execute(&mut *connection)
            .await
            .map_err(query(
                "allocate one wrapped key and nonce per missing object",
            ))?;
        sqlx::query(include_str!("sql/plan_bytes.sql"))
            .bind(&plan.id)
            .bind(new_bytes)
            .execute(&mut *connection)
            .await
            .map_err(query("account checkpoint's unique upload allowance"))?;
    }
    Ok(())
}

fn prepare(
    tasks: &Tasks,
    storage: &R2CheckpointStore,
    plan: &Plan,
    workspace: &Uuid7,
    identity: &Uuid7,
    descriptor: &CheckpointPlaintext,
    content: &str,
) -> Result<NewObject> {
    crate::task::storage::registration::prepare(
        &tasks.leases,
        storage,
        crate::task::storage::registration::Registration {
            workspace,
            namespace: &plan.repository,
            identity,
            format: plan.format_version,
        },
        descriptor,
        content,
    )
}

pub(super) async fn attach(
    connection: &mut PgConnection,
    plan: &Plan,
    objects: &[Uuid7],
) -> Result<()> {
    sqlx::query(include_str!("sql/objects_attach.sql"))
        .bind(objects)
        .bind(&plan.workspace_id)
        .bind((plan.mode()? == af_wire::task::CheckpointMode::Incremental).then_some(&plan.id))
        .bind(
            if plan.mode()? == af_wire::task::CheckpointMode::RetainBase {
                plan.base_id.as_ref()
            } else {
                None
            },
        )
        .execute(connection)
        .await
        .map_err(query(
            "protect registered objects with owner-scoped references",
        ))?;
    Ok(())
}
