//! Bounded registration, immutable ciphertext and native reconciliation share one pin.

use super::{
    Object, Session,
    uploads::{self, UploadBatch},
};
use crate::{Result, error};
use af_chunk_store::READ_WINDOW;
use af_core::{clock, id::Uuid7};
use af_wire::durable_task::{
    CheckpointAuthorizationInput, CheckpointObjectsInput, CheckpointPinInput, CheckpointPlaintext,
    CheckpointTransferPurpose, CheckpointUploadsInput,
};
use futures_util::{StreamExt as _, stream};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// One object's plaintext, shared by every place a batch names it.
pub(super) type Chunk = std::sync::Arc<af_crypto::secret::SecretBytes>;

impl Session<'_> {
    pub(super) async fn renew(&mut self) -> Result<()> {
        if self.stop.is_cancelled() || self.plan.pin.operation_deadline <= clock::now() {
            return Err(error::config("checkpoint custody ended"));
        }
        if clock::elapsed(clock::now(), self.plan.pin.expires_at)
            > std::time::Duration::from_secs(90)
        {
            return Ok(());
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let pin = Uuid7::parse(&self.plan.pin.id)?;
        let renewed = self
            .plane
            .durable_task_checkpoint_pin(
                self.run,
                &checkpoint,
                &pin,
                &CheckpointPinInput {
                    fencing_token: self.fence,
                    durable_task: self.task.clone(),
                    resource_version: self.plan.pin.resource_version,
                },
            )
            .await?;
        if renewed.id != self.plan.pin.id
            || renewed.operation_deadline != self.plan.pin.operation_deadline
        {
            return Err(error::config("checkpoint renewal changed native custody"));
        }
        self.plan.pin = renewed;
        Ok(())
    }

    async fn register_plaintext(
        &mut self,
        descriptors: Vec<CheckpointPlaintext>,
    ) -> Result<Vec<Object>> {
        self.renew().await?;
        if descriptors.is_empty() || descriptors.len() > 256 {
            return Err(error::config("checkpoint registration batch is invalid"));
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let response = self
            .plane
            .durable_task_checkpoint_objects_register(
                self.run,
                &checkpoint,
                &CheckpointObjectsInput {
                    fencing_token: self.fence,
                    durable_task: self.task.clone(),
                    pin_id: self.plan.pin.id.clone(),
                    objects: descriptors.clone(),
                },
            )
            .await?;
        let objects: Vec<Object> = response.items.into_iter().map(Object::from).collect();
        if objects.len() != descriptors.len()
            || response.next_cursor.is_some()
            || objects
                .iter()
                .zip(&descriptors)
                .any(|(object, descriptor)| {
                    object.repository != self.plan.repository
                        || object.plaintext.sha256 != descriptor.sha256
                        || object.plaintext.size != descriptor.size
                        || object.plaintext.kind != descriptor.kind
                })
        {
            return Err(error::config(
                "checkpoint registration differs from requested plaintext",
            ));
        }
        Ok(objects)
    }

    /// Seals a batch, freezes its ciphertext descriptors with the daemon, and
    /// PUTs it; the objects the bucket accepted. Memory is bounded separately;
    /// at most four physical requests run at once.
    async fn upload_sealed(
        &mut self,
        pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> Result<Vec<String>> {
        self.renew().await?;
        let workspace = self.workspace.clone();
        let mut batch =
            tokio::task::spawn_blocking(move || UploadBatch::seal(&workspace, pending)).await??;
        self.renew().await?;
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let response = self
            .plane
            .durable_task_checkpoint_uploads(
                self.run,
                &checkpoint,
                &CheckpointUploadsInput {
                    fencing_token: self.fence,
                    durable_task: self.task.clone(),
                    pin_id: self.plan.pin.id.clone(),
                    objects: batch.descriptors.clone(),
                },
            )
            .await?;
        let accepted = batch
            .put(response.items, &self.plan.repository, &self.transfers)
            .await?;
        for id in &accepted {
            self.progress.acknowledge(id);
        }
        self.progress(
            af_wire::activity::DurableTaskActivityOperation::Capture,
            af_wire::activity::DurableTaskActivityPhase::Uploading,
            None,
        )
        .await;
        Ok(accepted)
    }

    async fn restore_objects(&mut self, ids: &[String]) -> Result<Vec<Object>> {
        if ids.is_empty() || ids.len() > READ_WINDOW {
            return Err(error::config("checkpoint restore batch exceeds its bound"));
        }
        self.renew().await?;
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let unique: std::collections::BTreeSet<_> = ids.iter().cloned().collect();
        let response = self
            .plane
            .durable_task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    durable_task: self.task.clone(),
                    purpose: CheckpointTransferPurpose::Restore,
                    pin_id: Some(self.plan.pin.id.clone()),
                    object_ids: unique.iter().cloned().collect(),
                },
            )
            .await?;
        let objects: Vec<Object> = response.objects.into_iter().map(Object::from).collect();
        let returned: std::collections::BTreeSet<_> = objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect();
        if objects.len() != unique.len()
            || returned != unique
            || objects
                .iter()
                .any(|object| object.repository != self.plan.repository)
        {
            return Err(error::config(
                "checkpoint restore authorization differs from requested objects",
            ));
        }
        for object in &objects {
            let length = af_chunk_store::stored_length(object)?;
            self.progress.discover(&object.object_id, length, false);
        }
        Ok(objects)
    }

    /// Authorizes one window of objects once, then fetches them together.
    /// The result preserves manifest order, including repeated content chunks.
    pub(super) async fn download_batch(&mut self, ids: &[String]) -> Result<Vec<Chunk>> {
        let objects = self.restore_objects(ids).await?;
        self.progress(
            af_wire::activity::DurableTaskActivityOperation::Restore,
            af_wire::activity::DurableTaskActivityPhase::Restoring,
            None,
        )
        .await;
        let transfers = &self.transfers;
        let workspace = self.workspace;
        let outcomes = stream::iter(objects.into_iter().map(async move |mut object| {
            let bytes = transfers.download(&object).await?;
            let bytes = af_chunk_store::open(workspace, &mut object, bytes)?;
            Ok::<_, crate::Error>((object.object_id.clone(), bytes))
        }))
        .buffer_unordered(READ_WINDOW)
        .collect::<Vec<_>>()
        .await;
        let mut restored = BTreeMap::new();
        let mut failure = None;
        for outcome in outcomes {
            match outcome {
                Ok((id, bytes)) => {
                    self.progress.acknowledge(&id);
                    restored.insert(id, std::sync::Arc::new(bytes));
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
        self.progress(
            af_wire::activity::DurableTaskActivityOperation::Restore,
            af_wire::activity::DurableTaskActivityPhase::Restoring,
            None,
        )
        .await;
        if let Some(error) = failure {
            return Err(error);
        }
        ids.iter()
            .map(|id| {
                restored
                    .get(id)
                    .cloned()
                    .ok_or_else(|| error::config("checkpoint restore object is absent"))
            })
            .collect()
    }
}

impl uploads::Publisher for Session<'_> {
    async fn register(&mut self, descriptors: Vec<CheckpointPlaintext>) -> Result<Vec<Object>> {
        self.register_plaintext(descriptors).await
    }

    async fn registered(&mut self, objects: &[Object], total: usize) {
        for object in objects {
            if !matches!(
                object.status,
                af_wire::durable_task::CheckpointObjectStatus::Busy
            ) {
                self.progress.discover(
                    &object.object_id,
                    uploads::ciphertext_size(object),
                    matches!(
                        object.status,
                        af_wire::durable_task::CheckpointObjectStatus::Reused
                    ),
                );
            }
        }
        let total = u32::try_from(total).ok();
        self.progress(
            af_wire::activity::DurableTaskActivityOperation::Capture,
            af_wire::activity::DurableTaskActivityPhase::Uploading,
            total,
        )
        .await;
    }

    async fn upload_batch(
        &mut self,
        pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> Result<Vec<String>> {
        self.upload_sealed(pending).await
    }
}
