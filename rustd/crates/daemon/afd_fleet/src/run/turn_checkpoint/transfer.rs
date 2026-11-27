//! Freeze transfer authority under SQL locks, then sign outside them.

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun,
    files::FileOwner,
    guard, objects,
    signing::{self, Batch},
    transaction,
};
use crate::{
    Result,
    durable_task::checkpoint::{self, StoredObject},
    error::query,
    run::Runs,
};
use af_core::id::Uuid7;
use af_wire::{
    durable_task::{CheckpointTransferPurpose, CheckpointUpload},
    run::{CheckpointOperationId, TurnCheckpointRef, TurnCheckpointResponse},
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

/// The two transfer verbs: uploads freeze the ciphertext the runner will send,
/// and an authorization restores what is already stored.
pub(super) enum Transfer<'a> {
    Uploads(&'a [CheckpointUpload]),
    Authorize(CheckpointTransferPurpose, &'a [CheckpointOperationId]),
}

pub(super) async fn execute(
    store: &TurnCheckpoints,
    runs: &Runs,
    presented: TurnRun<'_>,
    reference: &TurnCheckpointRef,
    transfer: Transfer<'_>,
) -> Result<TurnCheckpointResponse> {
    let (purpose, ids) = match transfer {
        Transfer::Uploads(input) => (CheckpointTransferPurpose::Upload, upload_ids(input)?),
        // Nothing is read back: an authorization only ever restores.
        Transfer::Authorize(CheckpointTransferPurpose::Restore, object_ids) => (
            CheckpointTransferPurpose::Restore,
            objects::ids(object_ids)?,
        ),
        Transfer::Authorize(CheckpointTransferPurpose::Upload, _) => {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(runs, &mut connection, presented, reference, purpose).await?;
    freeze_uploads(&mut custody, &transfer, &ids).await?;
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
    let items = signing::issue(store.files()?, batch, purpose).await?;
    Ok(match purpose {
        CheckpointTransferPurpose::Upload => TurnCheckpointResponse::Uploads(items),
        CheckpointTransferPurpose::Restore => TurnCheckpointResponse::Authorized(items),
    })
}

async fn freeze_uploads(
    custody: &mut FileOwner<'_>,
    transfer: &Transfer<'_>,
    ids: &[Uuid7],
) -> Result<()> {
    let registered = objects::scoped(custody, ids).await?;
    if let Transfer::Uploads(input) = transfer {
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
    sqlx::query(checkpoint::DESCRIPTORS)
        .bind(Json(values))
        .bind(&custody.owner.current.workspace_id)
        .execute(&mut *custody.owner.tx)
        .await
        .map_err(query("freeze immutable turn ciphertext descriptors"))?;
    Ok(())
}
