//! Freeze transfer authority under SQL locks; remote verification owns no transaction.

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun,
    files::FileOwner,
    guard, objects,
    signing::{self, Batch},
    transaction,
};
use crate::{
    Result,
    error::query,
    run::Runs,
    task::checkpoint::{self, StoredObject, Verification, lock_quota},
};
use af_core::id::Uuid7;
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointRef,
        TurnCheckpointResponse,
    },
    task::{CheckpointTransferPurpose, CheckpointUpload},
};
use serde::Serialize;
use sqlx::types::Json;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct Descriptor<'a> {
    id: &'a Uuid7,
    digest: &'a str,
    length: i64,
}

pub(super) async fn execute(
    store: &TurnCheckpoints,
    runs: &Runs,
    presented: TurnRun<'_>,
    request: &TurnCheckpointCommand,
) -> Result<TurnCheckpointResponse> {
    let (reference, purpose, ids, verifying) = match &request.action {
        TurnCheckpointAction::Uploads {
            checkpoint,
            objects: input,
        } => {
            let ids = upload_ids(input)?;
            (checkpoint, CheckpointTransferPurpose::Upload, ids, false)
        }
        TurnCheckpointAction::Authorize {
            checkpoint,
            purpose,
            object_ids,
        } => (checkpoint, *purpose, objects::ids(object_ids)?, true),
        _ => return Err(TurnCheckpointRefusal::Incompatible.into()),
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(runs, &mut connection, presented, reference, purpose).await?;
    freeze_uploads(&mut custody, request, &ids).await?;
    let registered = objects::scoped(&mut custody, &ids).await?;
    objects::protect(&mut custody, &ids).await?;
    let pin = custody.pin.wire();
    let batch = Batch {
        workspace: custody.owner.current.workspace_id.clone(),
        namespace: custody.namespace.clone(),
        checkpoint: custody.plan.id.clone(),
        writer: custody.plan.owner_run_id.clone(),
        fence: custody.plan.owner_fence,
        format: custody.plan.format_version,
        deadline: pin.expires_at.min(pin.operation_deadline),
        objects: registered,
    };
    custody.commit().await?;
    transaction::recycle(connection);
    let (items, receipts) = signing::issue(store.files()?, batch, purpose, verifying).await?;
    confirm(runs, presented, reference, purpose, &ids, &receipts).await?;
    Ok(if verifying {
        TurnCheckpointResponse::Authorized(items)
    } else {
        TurnCheckpointResponse::Uploads(items)
    })
}

async fn freeze_uploads(
    custody: &mut FileOwner<'_>,
    request: &TurnCheckpointCommand,
    ids: &[Uuid7],
) -> Result<()> {
    let registered = objects::scoped(custody, ids).await?;
    if let TurnCheckpointAction::Uploads { objects: input, .. } = &request.action {
        freeze(custody, &registered, input).await?;
    }
    Ok(())
}

fn upload_ids(input: &[CheckpointUpload]) -> Result<Vec<Uuid7>> {
    input
        .iter()
        .map(|object| guard::id(&CheckpointOperationId(object.object_id.clone())))
        .collect()
}

async fn freeze(
    custody: &mut FileOwner<'_>,
    objects: &[StoredObject],
    input: &[CheckpointUpload],
) -> Result<()> {
    let by_id: BTreeMap<_, _> = objects
        .iter()
        .map(|object| (object.id.as_str(), object))
        .collect();
    let mut values = Vec::with_capacity(input.len());
    for descriptor in input {
        let saved = by_id
            .get(descriptor.object_id.as_str())
            .ok_or(TurnCheckpointRefusal::Conflict)?;
        let length = i64::try_from(descriptor.ciphertext_length)
            .map_err(|_overflow| TurnCheckpointRefusal::Limit)?;
        if !saved.verified
            && !saved.belongs(
                &custody.plan.id,
                &custody.owner.current.id,
                custody.owner.current.fence,
            )
            || saved.accounted_bytes != length
            || saved
                .ciphertext_digest
                .as_ref()
                .is_some_and(|digest| digest != &descriptor.ciphertext_sha256)
            || saved.ciphertext_length.is_some_and(|saved| saved != length)
        {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        values.push(Descriptor {
            id: &saved.id,
            digest: &descriptor.ciphertext_sha256,
            length,
        });
    }
    if by_id.len() != values.len() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    sqlx::query(checkpoint::DESCRIPTORS)
        .bind(Json(values))
        .bind(&custody.owner.current.workspace_id)
        .execute(&mut *custody.owner.tx)
        .await
        .map_err(query("freeze immutable turn ciphertext descriptors"))?;
    Ok(())
}

async fn confirm(
    runs: &Runs,
    presented: TurnRun<'_>,
    reference: &TurnCheckpointRef,
    purpose: CheckpointTransferPurpose,
    ids: &[Uuid7],
    receipts: &[Verification],
) -> Result<()> {
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(runs, &mut connection, presented, reference, purpose).await?;
    if !receipts.is_empty() {
        lock_quota(&mut custody.owner.tx, &custody.owner.current.workspace_id).await?;
    }
    objects::scoped(&mut custody, ids).await?;
    if !receipts.is_empty() {
        sqlx::query(checkpoint::VERIFY)
            .bind(&custody.owner.current.workspace_id)
            .bind(Json(receipts))
            .execute(&mut *custody.owner.tx)
            .await
            .map_err(query("account authenticated turn objects once"))?;
    }
    custody.commit().await?;
    transaction::recycle(connection);
    Ok(())
}
