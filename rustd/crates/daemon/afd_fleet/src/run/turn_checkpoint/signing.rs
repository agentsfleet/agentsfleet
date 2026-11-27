//! Bounded remote transfers use only committed custody and an absolute pin deadline.

use super::TurnCheckpointRefusal;
use crate::{
    Result,
    durable_task::{
        checkpoint::{StoredObject, Verification, verify_object},
        storage::{CheckpointKeys, Held, R2CheckpointStore, StorageSession, WorkspaceKey},
    },
};
use af_core::{clock, id::Uuid7};
use af_crypto::checkpoint::CheckpointScope;
use af_wire::durable_task::{
    CheckpointObject, CheckpointObjectStatus, CheckpointTransfer, CheckpointTransferPurpose,
};
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

/// One batch's frames carry the workspace's key, loaded once. A restore whose
/// key is missing or will not open reads nothing.
pub(super) async fn issue(
    storage: &R2CheckpointStore,
    mut batch: Batch,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
) -> Result<(Vec<CheckpointObject>, Vec<Verification>)> {
    if batch.objects.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let Held::Usable(key) = storage.keys().held(&batch.workspace).await? else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let session = storage.session().await?;
    let objects = std::mem::take(&mut batch.objects);
    let signing = Signing {
        storage,
        session: &session,
        batch: &batch,
        key: &key,
        purpose,
        verifying,
    };
    let outcomes = stream::iter(objects.into_iter().map(|object| signing.object(object)))
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

struct Signing<'a, 'store> {
    storage: &'a R2CheckpointStore,
    session: &'a StorageSession<'store>,
    batch: &'a Batch,
    key: &'a WorkspaceKey,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
}

impl Signing<'_, '_> {
    async fn object(
        &self,
        object: StoredObject,
    ) -> Result<(CheckpointObject, Option<Verification>)> {
        let (batch, purpose) = (self.batch, self.purpose);
        admitted(batch, &object, purpose)?;
        if purpose == CheckpointTransferPurpose::Restore && !object.sealed_under(self.key) {
            CheckpointKeys::mismatched(&batch.workspace);
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
        let id = object.id.clone();
        let verify =
            self.verifying && purpose == CheckpointTransferPurpose::Upload && !object.verified;
        let mut wire = object.described(self.storage, &batch.namespace)?;
        let receipt = if verify {
            tokio::time::timeout(
                clock::elapsed(clock::now(), batch.deadline),
                verify_object(
                    self.session,
                    CheckpointScope {
                        workspace_id: &batch.workspace,
                        repository: &batch.namespace,
                        object_id: &id,
                        format_version: batch.format,
                    },
                    &wire,
                    self.key,
                ),
            )
            .await
            .map_err(|_elapsed| TurnCheckpointRefusal::Conflict)??
        } else {
            None
        };
        if receipt.is_some() {
            wire.status = CheckpointObjectStatus::Reused;
        }
        wire.transfer = transfer(self.session, batch, &id, &wire, purpose).await?;
        wire.data_key = Some(self.key.wire());
        Ok((wire, receipt))
    }
}

fn admitted(
    batch: &Batch,
    object: &StoredObject,
    purpose: CheckpointTransferPurpose,
) -> Result<()> {
    if object.tombstoned
        || u32::try_from(object.format_version).ok() != Some(batch.format)
        || object.kind != "file"
        || !object.verified
            && (purpose == CheckpointTransferPurpose::Restore
                || !object.belongs(&batch.checkpoint, &batch.writer, batch.fence))
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

async fn transfer(
    session: &StorageSession<'_>,
    batch: &Batch,
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
            session.download(&batch.workspace, id, lifetime).await?
        }
    };
    Ok(Some(CheckpointTransfer {
        url: authorization.url,
        headers: authorization.headers,
        expires_at: clock::after(clock::now(), lifetime).min(batch.deadline),
    }))
}
