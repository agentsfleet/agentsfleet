//! Sign only committed custody; account successful verification even in a partial batch.

use super::{Batch, verify};
use crate::{
    Result,
    task::{
        TaskRefusal, Tasks,
        checkpoint::row::Plan,
        storage::{R2CheckpointStore, StorageSession},
    },
};
use af_core::clock;
use af_wire::task::{
    CheckpointObject, CheckpointObjectStatus, CheckpointTransfer, CheckpointTransferPurpose,
};
use futures_util::{StreamExt as _, stream};
use std::time::Duration;

pub(super) async fn issue(
    tasks: &Tasks,
    batch: Batch,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
) -> Result<Vec<CheckpointObject>> {
    let storage = super::super::guard::storage(tasks)?;
    let session = storage.session().await?;
    let signing = Signing {
        storage,
        session: &session,
        plan: &batch.plan,
        deadline: batch.pin.expires_at.min(batch.pin.operation_deadline),
        purpose,
        verifying,
    };
    let outcomes = stream::iter(
        batch
            .objects
            .into_iter()
            .map(|object| signing.object(object)),
    )
    .buffer_unordered(4)
    .collect::<Vec<_>>()
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
    verify::record(tasks, &batch.plan.workspace_id, &receipts).await?;
    objects.into_iter().collect()
}

struct Signing<'a, 'store> {
    storage: &'a R2CheckpointStore,
    session: &'a StorageSession<'store>,
    plan: &'a Plan,
    deadline: jiff::Timestamp,
    purpose: CheckpointTransferPurpose,
    verifying: bool,
}

impl Signing<'_, '_> {
    async fn object(
        &self,
        object: super::super::object_row::Object,
    ) -> Result<(CheckpointObject, Option<verify::Receipt>)> {
        let identity = object.identity_key_id.clone();
        let id = object.id.clone();
        if object.tombstoned
            || self.purpose == CheckpointTransferPurpose::Upload
                && !object.verified
                && !object.own(self.plan)
        {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let native_verified =
            self.verifying && self.purpose == CheckpointTransferPurpose::Upload && !object.verified;
        let (mut wire, key) = object.open(self.storage, self.plan)?;
        let receipt = if native_verified {
            verify::remote(
                self.session,
                &self.plan.workspace_id,
                &identity,
                &id,
                self.plan,
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
        wire.transfer = self.transfer(&identity, &id, &wire).await?;
        wire.data_key = Some(base64::Engine::encode(
            &base64::prelude::BASE64_STANDARD,
            key.expose(),
        ));
        Ok((wire, receipt))
    }

    async fn transfer(
        &self,
        identity: &af_core::id::Uuid7,
        id: &af_core::id::Uuid7,
        wire: &CheckpointObject,
    ) -> Result<Option<CheckpointTransfer>> {
        let lifetime = clock::elapsed(clock::now(), self.deadline)
            .min(Duration::from_secs(super::super::URL_SECONDS));
        if lifetime.is_zero() {
            return Err(TaskRefusal::StaleState.into());
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
                        identity,
                        id,
                        wire.ciphertext_sha256
                            .as_deref()
                            .ok_or(TaskRefusal::CheckpointIncompatible)?,
                        wire.ciphertext_length
                            .ok_or(TaskRefusal::CheckpointIncompatible)?,
                        lifetime,
                    )
                    .await?
            }
            CheckpointTransferPurpose::Restore => {
                self.session
                    .download(&self.plan.workspace_id, identity, id, lifetime)
                    .await?
            }
        };
        Ok(Some(CheckpointTransfer {
            url: authorization.url,
            headers: authorization.headers,
            expires_at: clock::now()
                .checked_add(lifetime)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?
                .min(self.deadline),
        }))
    }
}
