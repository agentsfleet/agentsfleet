//! One workspace storage lock and bounded set operations arbitrate upload owners.

use super::{guard, identity, object_row::Object, row::Plan};
use crate::durable_task::storage::R2CheckpointStore;
use crate::durable_task::storage::registration::NewObject;
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTasks, guard as durable_task_guard, transaction,
    },
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPlaintext, CheckpointTransferPurpose,
};
use sqlx::{PgConnection, types::Json};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::durable_task) async fn register(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointObjectsInput>,
) -> Result<CheckpointObjectsPage> {
    let storage = guard::storage(tasks)?;
    let pin = durable_task_guard::id(&input.pin_id, "pin_id")?;
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (_owned, plan, pin) = guard::owned(
        &mut tx,
        run,
        &input.durable_task,
        checkpoint,
        &pin,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    let items = registered(tasks, storage, &mut tx, &plan, &pin, input).await?;
    tx.commit()
        .await
        .map_err(query("commit registered object ownership"))?;
    transaction::recycle(connection);
    Ok(CheckpointObjectsPage {
        items,
        next_cursor: None,
    })
}

async fn registered(
    tasks: &DurableTasks,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    plan: &Plan,
    pin: &super::row::Pin,
    input: &CheckpointObjectsInput,
) -> Result<Vec<af_wire::durable_task::CheckpointObject>> {
    let (identity, content) = allocate(tasks, storage, connection, plan, input).await?;
    let objects = matching(connection, plan, &identity, &content).await?;
    let eligible: Vec<_> = objects
        .iter()
        .filter(|object| object.verified || object.own(plan))
        .map(|object| object.id.clone())
        .collect();
    attach(connection, plan, &eligible).await?;
    super::transfer::protect(connection, &eligible, pin, tasks.times().orphan_seconds()).await?;
    let mut by_content: BTreeMap<_, _> = objects
        .into_iter()
        .map(|object| (object.content_identity.clone(), object))
        .collect();
    content
        .iter()
        .map(|content| {
            by_content
                .remove(content)
                .ok_or(DurableTaskRefusal::CheckpointIncompatible)?
                .wire(storage, plan)
        })
        .collect()
}

async fn allocate(
    tasks: &DurableTasks,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    plan: &Plan,
    input: &CheckpointObjectsInput,
) -> Result<(Uuid7, Vec<String>)> {
    identity::lock_storage(connection, &plan.workspace_id).await?;
    let identity = identity::load(
        &tasks.runs,
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
        return Err(DurableTaskRefusal::ReplayConflict.into());
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
    reserve(
        connection,
        plan,
        &new,
        (storage, tasks.times().orphan_seconds()),
    )
    .await?;
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
    (storage, orphan_seconds): (&R2CheckpointStore, i32),
) -> Result<()> {
    let new_bytes = new
        .iter()
        .try_fold(0_i64, |sum, object| sum.checked_add(object.accounted_bytes))
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
    if plan
        .uploaded_bytes
        .checked_add(new_bytes)
        .is_none_or(|bytes| bytes > plan.upload_budget_bytes)
    {
        return Err(DurableTaskRefusal::LimitExhausted.into());
    }
    if !new.is_empty() {
        sqlx::query(super::OBJECTS_INSERT)
            .bind(Json(new))
            .bind(&plan.workspace_id)
            .bind(&plan.id)
            .bind(&plan.owner_run_id)
            .bind(plan.owner_fence)
            .bind(storage.bucket_reference())
            .bind(orphan_seconds)
            .execute(&mut *connection)
            .await
            .map_err(query(
                "allocate one wrapped key and nonce per missing object",
            ))?;
        sqlx::query(super::PLAN_BYTES)
            .bind(&plan.id)
            .bind(new_bytes)
            .execute(&mut *connection)
            .await
            .map_err(query("account checkpoint's unique upload allowance"))?;
    }
    Ok(())
}

fn prepare(
    tasks: &DurableTasks,
    storage: &R2CheckpointStore,
    plan: &Plan,
    workspace: &Uuid7,
    identity: &Uuid7,
    descriptor: &CheckpointPlaintext,
    content: &str,
) -> Result<NewObject> {
    crate::durable_task::storage::registration::prepare(
        &tasks.runs,
        storage,
        crate::durable_task::storage::registration::Registration {
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
        .bind(
            (plan.mode()? == af_wire::durable_task::CheckpointMode::Incremental)
                .then_some(&plan.id),
        )
        .bind(
            if plan.mode()? == af_wire::durable_task::CheckpointMode::RetainBase {
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
