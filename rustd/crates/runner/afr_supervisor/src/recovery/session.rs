//! Bounded registration, immutable ciphertext and native reconciliation share one pin.

use super::{
    Object, Session,
    catalog::Chunk,
    transfer,
    uploads::{self, UploadBatch},
};
use crate::{Result, error};
use af_core::{clock, id::Uuid7};
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointObjectsInput, CheckpointPinInput, CheckpointPlaintext,
    CheckpointTransferPurpose, CheckpointUpload, CheckpointUploadsInput,
};
use futures_util::{StreamExt as _, stream};
use std::collections::BTreeMap;

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
            .task_checkpoint_pin(
                self.run,
                &checkpoint,
                &pin,
                &CheckpointPinInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
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

    pub(super) async fn register(
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
            .task_checkpoint_objects_register(
                self.run,
                &checkpoint,
                &CheckpointObjectsInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
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

    /// A native page of immutable ciphertexts shares authorization and readback.
    /// Memory is bounded separately; at most four physical requests run at once.
    pub(super) async fn upload_batch(&mut self, pending: Vec<(Object, Chunk)>) -> Result<()> {
        self.renew().await?;
        let workspace = self.workspace.clone();
        let mut batch =
            tokio::task::spawn_blocking(move || UploadBatch::seal(&workspace, pending)).await??;
        self.renew().await?;
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let response = self
            .plane
            .task_checkpoint_uploads(
                self.run,
                &checkpoint,
                &CheckpointUploadsInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
                    pin_id: self.plan.pin.id.clone(),
                    objects: batch.descriptors.clone(),
                },
            )
            .await?;
        let outcomes = batch
            .put(response.items, &self.plan.repository, &self.transfers)
            .await?;
        self.verify_uploads(&batch.descriptors, outcomes).await
    }

    /// Reconcile physical outcomes with native immutable ciphertext receipts.
    async fn verify_uploads(
        &mut self,
        descriptors: &[CheckpointUpload],
        outcomes: Vec<Result<()>>,
    ) -> Result<()> {
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        // Even an unknown PUT is read back once. It is never resent blindly.
        let verified = self
            .plane
            .task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
                    purpose: CheckpointTransferPurpose::Upload,
                    pin_id: Some(self.plan.pin.id.clone()),
                    object_ids: descriptors
                        .iter()
                        .map(|object| object.object_id.clone())
                        .collect(),
                },
            )
            .await?;
        let objects: Vec<Object> = verified.objects.into_iter().map(Object::from).collect();
        let mut complete = true;
        for descriptor in descriptors {
            let confirmed = objects
                .iter()
                .any(|object| uploads::verified(object, descriptor, &self.plan.repository));
            if confirmed {
                self.progress.acknowledge(&descriptor.object_id);
            }
            complete &= confirmed;
        }
        self.progress(
            af_wire::activity::TaskActivityOperation::Capture,
            af_wire::activity::TaskActivityPhase::Uploading,
            None,
        )
        .await;
        if !complete {
            for outcome in outcomes {
                outcome?;
            }
            return Err(error::config(
                "checkpoint upload batch was not fully verified",
            ));
        }
        Ok(())
    }

    async fn restore_objects(&mut self, ids: &[String]) -> Result<Vec<Object>> {
        if ids.is_empty() || ids.len() > 4 {
            return Err(error::config("checkpoint restore batch exceeds its bound"));
        }
        self.renew().await?;
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let unique: std::collections::BTreeSet<_> = ids.iter().cloned().collect();
        let response = self
            .plane
            .task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
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
            let length = object
                .ciphertext_length
                .filter(|length| {
                    (17..=super::files::CHUNK + crate::byte_count(af_crypto::TAG_LEN))
                        .contains(length)
                })
                .ok_or_else(|| error::config("checkpoint ciphertext length is invalid"))?;
            self.progress.discover(&object.object_id, length, false);
        }
        Ok(objects)
    }

    /// Authorize at most four unique objects once, then restore concurrently.
    /// The result preserves manifest order, including repeated content chunks.
    pub(super) async fn download_batch(
        &mut self,
        ids: &[String],
    ) -> Result<Vec<super::window::Chunk>> {
        let objects = self.restore_objects(ids).await?;
        self.progress(
            af_wire::activity::TaskActivityOperation::Restore,
            af_wire::activity::TaskActivityPhase::Restoring,
            None,
        )
        .await;
        let transfers = &self.transfers;
        let workspace = self.workspace;
        let outcomes = stream::iter(objects.into_iter().map(async move |mut object| {
            let bytes = transfers.download(&object).await?;
            let bytes = transfer::open(workspace, &mut object, bytes)?;
            Ok::<_, crate::Error>((object.object_id.clone(), bytes))
        }))
        .buffer_unordered(4)
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
            af_wire::activity::TaskActivityOperation::Restore,
            af_wire::activity::TaskActivityPhase::Restoring,
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
