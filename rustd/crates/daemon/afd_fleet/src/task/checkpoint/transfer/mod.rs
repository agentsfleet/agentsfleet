//! Persist exact transfer authority, release SQL locks, then sign or verify in parallel.

mod verify;

use super::{
    guard,
    object_row::Object,
    row::{Pin, Plan},
};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, execution, guard as task_guard, transaction},
};
use af_core::{clock, id::Uuid7};
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointAuthorizations, CheckpointObject,
    CheckpointObjectsPage, CheckpointReadQuery, CheckpointTransfer, CheckpointTransferPurpose,
    CheckpointUploadsInput,
};
use futures_util::{StreamExt as _, stream};
use jiff_sqlx::ToSqlx as _;
use serde::Serialize;
use sqlx::{PgConnection, types::Json};
use std::{collections::BTreeSet, time::Duration};

struct Batch {
    plan: Plan,
    pin: Pin,
    objects: Vec<Object>,
}

#[derive(Serialize)]
struct Descriptor<'a> {
    id: &'a str,
    digest: &'a str,
    length: i64,
}

pub(in crate::task) async fn uploads(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointUploadsInput>,
) -> Result<CheckpointObjectsPage> {
    let ids = input
        .objects
        .iter()
        .map(|object| task_guard::wire(&object.object_id))
        .collect::<Result<Vec<_>>>()?;
    unique(&ids)?;
    let pin_id = task_guard::wire(&input.pin_id)?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (_owned, plan, pin) = guard::owned(
        &mut tx,
        lease,
        &input.task,
        checkpoint,
        &pin_id,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    let objects = scoped(&mut tx, &plan, &ids).await?;
    let mut descriptors = Vec::with_capacity(input.objects.len());
    for input in &input.objects {
        let object = objects
            .iter()
            .find(|object| object.id == input.object_id)
            .ok_or(TaskRefusal::NotFound)?;
        let length = i64::try_from(input.ciphertext_length)
            .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?;
        if object.tombstoned
            || !object.own(&plan) && !object.verified
            || length != object.accounted_bytes
            || object
                .ciphertext_digest
                .as_ref()
                .is_some_and(|digest| digest != &input.ciphertext_sha256)
            || object
                .ciphertext_length
                .is_some_and(|saved| saved != length)
        {
            return Err(TaskRefusal::ReplayConflict.into());
        }
        descriptors.push(Descriptor {
            id: &input.object_id,
            digest: &input.ciphertext_sha256,
            length,
        });
    }
    sqlx::query(include_str!("../sql/descriptors.sql"))
        .bind(Json(&descriptors))
        .bind(&plan.workspace_id)
        .execute(&mut *tx)
        .await
        .map_err(query(
            "freeze complete ciphertext descriptors before URL exposure",
        ))?;
    let objects = scoped(&mut tx, &plan, &ids).await?;
    protect(&mut tx, &ids, &pin).await?;
    tx.commit()
        .await
        .map_err(query("commit immutable create-only upload authority"))?;
    transaction::recycle(connection);
    let objects = issue(
        tasks,
        Batch { plan, pin, objects },
        CheckpointTransferPurpose::Upload,
        false,
    )
    .await?;
    Ok(CheckpointObjectsPage {
        items: objects,
        next_cursor: None,
    })
}

pub(in crate::task) async fn authorize(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointAuthorizationInput>,
) -> Result<CheckpointAuthorizations> {
    let object_ids = input
        .object_ids
        .iter()
        .map(|id| task_guard::wire(id))
        .collect::<Result<Vec<_>>>()?;
    unique(&object_ids)?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (plan, pin) = if let Some(pin) = &input.pin_id {
        let (_owned, plan, pin) = guard::owned(
            &mut tx,
            lease,
            &input.task,
            checkpoint,
            &task_guard::id(pin, "pin_id")?,
            input.purpose,
        )
        .await?;
        (plan, pin)
    } else {
        if input.purpose != CheckpointTransferPurpose::Restore || !input.object_ids.is_empty() {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let owned = execution::load(&mut tx, lease, &input.task).await?;
        let plan = Plan::load(
            &mut tx,
            checkpoint,
            owned
                .current
                .task_id
                .as_ref()
                .ok_or(TaskRefusal::NotFound)?,
        )
        .await?;
        plan.compatible(&owned, CheckpointTransferPurpose::Restore)?;
        let pin = super::pins::insert(tasks, &mut tx, &owned, &plan, lease, "reader").await?;
        (plan, pin)
    };
    let objects = scoped(&mut tx, &plan, &object_ids).await?;
    if input.purpose == CheckpointTransferPurpose::Restore
        && objects.iter().any(|object| !object.verified)
    {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    protect(&mut tx, &object_ids, &pin).await?;
    let response = plan.wire(&pin, false)?;
    tx.commit()
        .await
        .map_err(query("commit scoped transfer refresh or restore bootstrap"))?;
    transaction::recycle(connection);
    let objects = issue(tasks, Batch { plan, pin, objects }, input.purpose, true).await?;
    Ok(CheckpointAuthorizations {
        plan: response,
        objects,
    })
}

pub(in crate::task) async fn list(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointReadQuery>,
) -> Result<CheckpointObjectsPage> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = super::page::read_authority(&mut tx, lease, checkpoint, &input.pin_id).await?;
    let (_current, plan, pin) = owned;
    let after: Option<Uuid7> = input
        .starting_after
        .as_deref()
        .map(|id| task_guard::id(id, "starting_after"))
        .transpose()?;
    let mut ids: Vec<Uuid7> = sqlx::query_scalar(include_str!("../sql/object_ids.sql"))
        .bind(&plan.id)
        .bind(&plan.base_id)
        .bind(&plan.workspace_id)
        .bind(&after)
        .bind(101_i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("page the scoped restore object set"))?;
    let more = ids.len() > 100;
    if more {
        ids.pop();
    }
    let next_cursor = more.then(|| ids.last().cloned()).flatten();
    let objects = scoped(&mut tx, &plan, &ids).await?;
    if objects.iter().any(|object| !object.verified) {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    protect(&mut tx, &ids, &pin).await?;
    tx.commit()
        .await
        .map_err(query("commit bounded restore transfer authority"))?;
    transaction::recycle(connection);
    let items = issue(
        tasks,
        Batch { plan, pin, objects },
        CheckpointTransferPurpose::Restore,
        false,
    )
    .await?;
    Ok(CheckpointObjectsPage {
        items,
        next_cursor: next_cursor.map(String::from),
    })
}

pub(super) async fn scoped(
    connection: &mut PgConnection,
    plan: &Plan,
    ids: &[Uuid7],
) -> Result<Vec<Object>> {
    let objects: Vec<Object> = sqlx::query_as(include_str!("../sql/objects_scope.sql"))
        .bind(ids)
        .bind(&plan.workspace_id)
        .bind(&plan.id)
        .bind(&plan.base_id)
        .bind(&plan.repository)
        .fetch_all(connection)
        .await
        .map_err(query("lock exact checkpoint/base object scope"))?;
    if objects.len() != ids.len() {
        return Err(TaskRefusal::NotFound.into());
    }
    Ok(objects)
}

pub(super) async fn protect(connection: &mut PgConnection, ids: &[Uuid7], pin: &Pin) -> Result<()> {
    sqlx::query(include_str!("../sql/objects_protect.sql"))
        .bind(ids)
        .bind(super::ORPHAN_SECONDS + 60)
        .bind(pin.expires_at.to_sqlx())
        .execute(connection)
        .await
        .map_err(query(
            "retain objects beyond all admitted URLs and in-flight requests",
        ))?;
    if pin.expires_at <= clock::now() || pin.operation_deadline <= clock::now() {
        return Err(TaskRefusal::StaleState.into());
    }
    Ok(())
}

async fn issue(
    tasks: &Tasks,
    batch: Batch,
    purpose: CheckpointTransferPurpose,
    verify: bool,
) -> Result<Vec<CheckpointObject>> {
    let storage = guard::storage(tasks)?;
    let session = storage.session().await?;
    let workspace = batch.plan.workspace_id.clone();
    let deadline = batch.pin.expires_at.min(batch.pin.operation_deadline);
    let plan = &batch.plan;
    let session = &session;
    let workspace = &workspace;
    let outcomes = stream::iter(batch.objects.into_iter().map(|object| async move {
        let identity = object.identity_key_id.clone();
        let id = object.id.clone();
        if object.tombstoned
            || purpose == CheckpointTransferPurpose::Upload && !object.verified && !object.own(plan)
        {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let native_verified =
            verify && purpose == CheckpointTransferPurpose::Upload && !object.verified;
        let (mut wire, key) = object.open(storage, plan)?;
        let receipt = if native_verified {
            verify::remote(session, workspace, &identity, &id, plan, &wire, &key).await?
        } else {
            None
        };
        if receipt.is_some() {
            wire.status = af_wire::task::CheckpointObjectStatus::Reused;
        }
        let lifetime =
            clock::elapsed(clock::now(), deadline).min(Duration::from_secs(super::URL_SECONDS));
        if lifetime.is_zero() {
            return Err(TaskRefusal::StaleState.into());
        }
        let authorization = match purpose {
            CheckpointTransferPurpose::Upload
                if matches!(wire.status, af_wire::task::CheckpointObjectStatus::Reused) =>
            {
                None
            }
            CheckpointTransferPurpose::Upload => Some(
                session
                    .upload(
                        workspace,
                        &identity,
                        &id,
                        wire.ciphertext_sha256
                            .as_deref()
                            .ok_or(TaskRefusal::CheckpointIncompatible)?,
                        wire.ciphertext_length
                            .ok_or(TaskRefusal::CheckpointIncompatible)?,
                        lifetime,
                    )
                    .await?,
            ),
            CheckpointTransferPurpose::Restore => Some(
                session
                    .download(workspace, &identity, &id, lifetime)
                    .await?,
            ),
        };
        if let Some(authorization) = authorization {
            wire.transfer = Some(CheckpointTransfer {
                url: authorization.url,
                headers: authorization.headers,
                expires_at: clock::now()
                    .checked_add(lifetime)
                    .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?
                    .min(deadline),
            });
        }
        wire.data_key = Some(base64::Engine::encode(
            &base64::prelude::BASE64_STANDARD,
            key.expose(),
        ));
        Ok((wire, receipt))
    }))
    .buffer_unordered(4)
    .collect::<Vec<Result<(CheckpointObject, Option<verify::Receipt>)>>>()
    .await;
    let mut receipts = Vec::new();
    let mut objects = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        objects.push(outcome.map(|(object, receipt)| {
            if let Some(receipt) = receipt {
                receipts.push(receipt);
            }
            object
        }));
    }
    verify::record(tasks, workspace, &receipts).await?;
    objects.into_iter().collect()
}

fn unique(ids: &[Uuid7]) -> Result<()> {
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    Ok(())
}
