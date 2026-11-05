//! One workspace quota lock and bounded set operations arbitrate upload owners.

use super::{guard, identity, object_row::Object, row::Plan};
use crate::task::storage::R2CheckpointStore;
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, guard as task_guard, transaction},
};
use afd_core::{clock, id::Uuid7};
use afd_crypto::{
    checkpoint::{CheckpointScope, PreparedCheckpointKey},
    envelope::Sealer,
};
use afd_wire::task::{
    CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPlaintext, CheckpointTransferPurpose,
};
use serde::Serialize;
use sqlx::{PgConnection, types::Json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
struct NewObject {
    id: String,
    identity_key_id: String,
    content_identity: String,
    plaintext_digest: String,
    plaintext_size: i64,
    kind: String,
    format_version: i32,
    accounted_bytes: i64,
    dek_ciphertext: String,
    dek_nonce: String,
    dek_tag: String,
    secret_nonce: String,
    secret_ciphertext: String,
    secret_tag: String,
    kek_version: i32,
    object_nonce: String,
}

pub(in crate::task) async fn register(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &afd_core::id::Uuid7,
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
    identity::quota(&mut tx, &plan.workspace_id).await?;
    let workspace = task_guard::id(&plan.workspace_id, "workspace_id")?;
    let identity = identity::load(tasks, storage, &mut tx, &workspace, &plan.repository).await?;
    let content: Vec<_> = input
        .objects
        .iter()
        .map(|object| identity.content(object, plan.format_version))
        .collect();
    if content.iter().collect::<BTreeSet<_>>().len() != content.len() {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let existing: Vec<Object> = sqlx::query_as(include_str!("sql/objects_content.sql"))
        .bind(&plan.workspace_id)
        .bind(identity.id.as_str())
        .bind(&content)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("lock matching immutable content batch"))?;
    let found: BTreeSet<_> = existing
        .iter()
        .map(|object| object.content_identity.as_str())
        .collect();
    let mut new = Vec::new();
    for (descriptor, content) in input
        .objects
        .iter()
        .zip(&content)
        .filter(|(_object, content)| !found.contains(content.as_str()))
    {
        new.push(prepare(
            tasks,
            storage,
            &plan,
            &workspace,
            &identity.id,
            descriptor,
            content,
        )?);
    }
    reserve(&mut tx, &plan, &new, storage).await?;
    let objects: Vec<Object> = sqlx::query_as(include_str!("sql/objects_content.sql"))
        .bind(&plan.workspace_id)
        .bind(identity.id.as_str())
        .bind(&content)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("read registered content batch"))?;
    let eligible: Vec<_> = objects
        .iter()
        .filter(|object| object.verified || object.own(&plan))
        .map(|object| object.id.clone())
        .collect();
    attach(&mut tx, &plan, &eligible).await?;
    super::transfer::protect(&mut tx, &eligible, &pin).await?;
    let mut by_content: BTreeMap<_, _> = objects
        .into_iter()
        .map(|object| (object.content_identity.clone(), object))
        .collect();
    let items = content
        .iter()
        .map(|content| {
            by_content
                .remove(content)
                .ok_or(TaskRefusal::CheckpointIncompatible)?
                .wire(storage, &plan)
        })
        .collect::<Result<_>>()?;
    tx.commit()
        .await
        .map_err(query("commit registered object ownership and quota"))?;
    transaction::recycle(connection);
    Ok(CheckpointObjectsPage {
        items,
        next_cursor: None,
    })
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
    let id = tasks.leases.entropy().uuid7(clock::now())?;
    let prepared = PreparedCheckpointKey::create(
        tasks.leases.entropy(),
        &Sealer::new(),
        &storage.master,
        CheckpointScope {
            workspace_id: workspace,
            repository: &plan.repository,
            object_id: &id,
            format_version: u32::try_from(plan.format_version)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        },
    )?;
    let (wrapped, key) = prepared.into_parts();
    drop(key);
    let envelope = wrapped.envelope();
    Ok(NewObject {
        id: id.as_str().to_owned(),
        identity_key_id: identity.as_str().to_owned(),
        content_identity: content.to_owned(),
        plaintext_digest: descriptor.sha256.clone(),
        plaintext_size: i64::try_from(descriptor.size)
            .map_err(|_overflow| TaskRefusal::LimitExhausted)?,
        kind: descriptor.kind.clone(),
        format_version: plan.format_version,
        accounted_bytes: i64::try_from(descriptor.size + afd_crypto::TAG_LEN as u64)
            .map_err(|_overflow| TaskRefusal::LimitExhausted)?,
        dek_ciphertext: pg_bytes(envelope.wrapped_dek()),
        dek_nonce: pg_bytes(envelope.dek_nonce()),
        dek_tag: pg_bytes(envelope.dek_tag()),
        secret_nonce: pg_bytes(envelope.payload_nonce()),
        secret_ciphertext: pg_bytes(envelope.payload_ciphertext()),
        secret_tag: pg_bytes(envelope.payload_tag()),
        kek_version: envelope.kek_version(),
        object_nonce: pg_bytes(wrapped.nonce()),
    })
}

pub(super) async fn attach(
    connection: &mut PgConnection,
    plan: &Plan,
    objects: &[String],
) -> Result<()> {
    sqlx::query(include_str!("sql/objects_attach.sql"))
        .bind(objects)
        .bind(&plan.workspace_id)
        .bind(
            (plan.mode()? == afd_wire::task::CheckpointMode::Incremental)
                .then_some(plan.id.as_str()),
        )
        .bind(
            if plan.mode()? == afd_wire::task::CheckpointMode::RetainBase {
                plan.base_id.as_deref()
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

fn pg_bytes(bytes: &[u8]) -> String {
    format!("\\x{}", hex::encode(bytes))
}
