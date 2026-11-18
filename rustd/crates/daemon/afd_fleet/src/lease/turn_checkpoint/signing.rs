//! Bounded remote transfers use only committed custody and an absolute pin deadline.

use super::TurnCheckpointRefusal;
use crate::{
    Result,
    task::{
        checkpoint::{StoredObject, Verification, verify_object},
        storage::{R2CheckpointStore, StorageSession},
    },
};
use af_core::{clock, id::Uuid7};
use af_crypto::checkpoint::CheckpointScope;
use af_wire::task::{
    CheckpointObject, CheckpointObjectStatus, CheckpointTransfer, CheckpointTransferPurpose,
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use futures_util::{StreamExt as _, stream};
use std::time::Duration;

pub(super) struct Batch {
    pub workspace: Uuid7,
    pub namespace: String,
    pub checkpoint: Uuid7,
    pub writer: Uuid7,
    pub fence: i64,
    pub format: u32,
    pub deadline: jiff::Timestamp,
    pub objects: Vec<StoredObject>,
}

pub(super) async fn issue(
    storage: &R2CheckpointStore,
    mut batch: Batch,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
) -> Result<(Vec<CheckpointObject>, Vec<Verification>)> {
    let session = storage.session().await?;
    let objects = std::mem::take(&mut batch.objects);
    let outcomes = stream::iter(
        objects
            .into_iter()
            .map(|object| issue_object(storage, &session, &batch, object, purpose, verifying)),
    )
    .buffered(4)
    .collect::<Vec<Result<(CheckpointObject, Option<Verification>)>>>()
    .await;
    let mut items = Vec::with_capacity(outcomes.len());
    let mut receipts = Vec::new();
    for outcome in outcomes {
        let (object, receipt) = outcome?;
        items.push(object);
        if let Some(receipt) = receipt {
            receipts.push(receipt);
        }
    }
    Ok((items, receipts))
}

async fn issue_object(
    storage: &R2CheckpointStore,
    session: &StorageSession<'_>,
    batch: &Batch,
    object: StoredObject,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
) -> Result<(CheckpointObject, Option<Verification>)> {
    let identity = object.identity_key_id.clone();
    let id = object.id.clone();
    if object.tombstoned
        || u32::try_from(object.format_version).ok() != Some(batch.format)
        || object.kind != "file"
        || !object.verified
            && (purpose == CheckpointTransferPurpose::Restore
                || !object.belongs(&batch.checkpoint, &batch.writer, batch.fence))
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let verify = verifying && purpose == CheckpointTransferPurpose::Upload && !object.verified;
    let (mut wire, key) = object.open_scoped(storage, &batch.workspace, &batch.namespace)?;
    let receipt = if verify {
        verify_object(
            session,
            &identity,
            CheckpointScope {
                workspace_id: &batch.workspace,
                repository: &batch.namespace,
                object_id: &id,
                format_version: batch.format,
            },
            &wire,
            &key,
        )
        .await?
    } else {
        None
    };
    if receipt.is_some() {
        wire.status = CheckpointObjectStatus::Reused;
    }
    wire.transfer = transfer(session, batch, &identity, &id, &wire, purpose).await?;
    wire.data_key = Some(BASE64_STANDARD.encode(key.expose()));
    Ok((wire, receipt))
}

async fn transfer(
    session: &StorageSession<'_>,
    batch: &Batch,
    identity: &Uuid7,
    id: &Uuid7,
    wire: &CheckpointObject,
    purpose: CheckpointTransferPurpose,
) -> Result<Option<CheckpointTransfer>> {
    let lifetime = clock::elapsed(clock::now(), batch.deadline).min(Duration::from_mins(5));
    if lifetime.is_zero() {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let authorization = match purpose {
        CheckpointTransferPurpose::Upload
            if matches!(wire.status, CheckpointObjectStatus::Reused) =>
        {
            return Ok(None);
        }
        CheckpointTransferPurpose::Upload => {
            session
                .upload(
                    &batch.workspace,
                    identity,
                    id,
                    wire.ciphertext_sha256
                        .as_deref()
                        .ok_or(TurnCheckpointRefusal::Incompatible)?,
                    wire.ciphertext_length
                        .ok_or(TurnCheckpointRefusal::Incompatible)?,
                    lifetime,
                )
                .await?
        }
        CheckpointTransferPurpose::Restore => {
            session
                .download(&batch.workspace, identity, id, lifetime)
                .await?
        }
    };
    Ok(Some(CheckpointTransfer {
        url: authorization.url,
        headers: authorization.headers,
        expires_at: clock::now()
            .checked_add(lifetime)
            .map_err(|_overflow| TurnCheckpointRefusal::Conflict)?
            .min(batch.deadline),
    }))
}
