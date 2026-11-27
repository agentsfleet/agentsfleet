//! Sign only committed custody.

use super::Batch;
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTasks,
        checkpoint::row::Plan,
        storage::{Held, R2CheckpointStore, StorageSession, WorkspaceKey},
    },
};
use af_core::clock;
use af_wire::durable_task::{
    CheckpointObject, CheckpointObjectStatus, CheckpointTransfer, CheckpointTransferPurpose,
};
use futures_util::{StreamExt as _, stream};
use std::time::Duration;

pub(super) async fn issue(
    tasks: &DurableTasks,
    batch: Batch,
    purpose: CheckpointTransferPurpose,
) -> Result<Vec<CheckpointObject>> {
    let storage = super::super::guard::storage(tasks)?;
    if batch.objects.is_empty() {
        // A restore's bootstrap names no object: nothing is signed, so
        // neither the key nor the bucket is read.
        return Ok(Vec::new());
    }
    let key = workspace_key(storage, &batch.plan, purpose).await?;
    let session = storage.session().await?;
    let signing = Signing {
        storage,
        key: &key,
        session: &session,
        plan: &batch.plan,
        deadline: batch.pin.expires_at.min(batch.pin.operation_deadline),
        url_lifetime: match purpose {
            CheckpointTransferPurpose::Upload => tasks.times().upload_link_ttl,
            CheckpointTransferPurpose::Restore => tasks.times().download_link_ttl,
        },
        purpose,
    };
    stream::iter(
        batch
            .objects
            .into_iter()
            .map(|object| signing.object(object)),
    )
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect()
}

/// The key the batch's frames carry. A restore whose key is missing, will not
/// open or did not seal these objects reads nothing.
async fn workspace_key(
    storage: &R2CheckpointStore,
    plan: &Plan,
    purpose: CheckpointTransferPurpose,
) -> Result<WorkspaceKey> {
    let workspace = &plan.workspace_id;
    match (storage.keys().held(workspace).await?, purpose) {
        (Held::Usable(key), _) => Ok(key),
        (Held::Unusable, CheckpointTransferPurpose::Restore) => {
            storage.keys().retire(workspace).await?;
            Err(DurableTaskRefusal::CheckpointIncompatible.into())
        }
        (Held::Unusable, CheckpointTransferPurpose::Upload) => {
            Err(DurableTaskRefusal::StorageUnavailable.into())
        }
    }
}

struct Signing<'a, 'store> {
    storage: &'a R2CheckpointStore,
    key: &'a WorkspaceKey,
    session: &'a StorageSession<'store>,
    plan: &'a Plan,
    deadline: jiff::Timestamp,
    url_lifetime: Duration,
    purpose: CheckpointTransferPurpose,
}

impl Signing<'_, '_> {
    async fn object(&self, object: super::super::object_row::Object) -> Result<CheckpointObject> {
        let id = object.id.clone();
        if object.tombstoned
            || self.purpose == CheckpointTransferPurpose::Upload
                && !object.verified
                && !object.own(self.plan)
        {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        if self.purpose == CheckpointTransferPurpose::Restore && !object.sealed_under(self.key) {
            crate::durable_task::storage::CheckpointKeys::mismatched(&self.plan.workspace_id);
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let mut wire = object.described(self.storage, &self.plan.repository)?;
        wire.transfer = self.transfer(&id, &wire).await?;
        wire.data_key = Some(self.key.wire());
        Ok(wire)
    }

    async fn transfer(
        &self,
        id: &af_core::id::Uuid7,
        wire: &CheckpointObject,
    ) -> Result<Option<CheckpointTransfer>> {
        let lifetime = clock::elapsed(clock::now(), self.deadline).min(self.url_lifetime);
        if lifetime.is_zero() {
            return Err(DurableTaskRefusal::StaleState.into());
        }
        let authorization = match self.purpose {
            CheckpointTransferPurpose::Upload
                if matches!(wire.status, CheckpointObjectStatus::Reused) =>
            {
                return Ok(None);
            }
            CheckpointTransferPurpose::Upload => {
                self.session
                    .upload(
                        &self.plan.workspace_id,
                        id,
                        wire.ciphertext_sha256
                            .as_deref()
                            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?,
                        wire.ciphertext_length
                            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?,
                        lifetime,
                    )
                    .await?
            }
            CheckpointTransferPurpose::Restore => {
                self.session
                    .download(&self.plan.workspace_id, id, lifetime)
                    .await?
            }
        };
        Ok(Some(CheckpointTransfer {
            url: authorization.url,
            headers: authorization.headers,
            expires_at: clock::after(clock::now(), lifetime).min(self.deadline),
        }))
    }
}
