//! Bounded registration, immutable ciphertext and native reconciliation share one pin.

use super::{Object, Session, transfer};
use crate::{Result, error};
use afd_core::{clock, id::Uuid7};
use afd_wire::task::{
    CHECKPOINT_BATCH, CheckpointAuthorizationInput, CheckpointObjectStatus, CheckpointObjectsInput,
    CheckpointPinInput, CheckpointPlaintext, CheckpointTransferPurpose, CheckpointUpload,
    CheckpointUploadsInput,
};
use futures_util::{StreamExt as _, stream};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

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
                self.lease,
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
                self.lease,
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
    pub(super) async fn upload_batch(
        &mut self,
        batch: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> Result<()> {
        let bytes = batch.iter().fold(0_u64, |sum, (_, bytes)| {
            sum.saturating_add(bytes.len() as u64 + afd_crypto::TAG_LEN as u64)
        });
        if batch.is_empty() || batch.len() > CHECKPOINT_BATCH || bytes > super::UPLOAD_BATCH_BYTES {
            return Err(error::config("checkpoint physical batch exceeds its bound"));
        }
        self.renew().await?;
        let mut ciphertexts = BTreeMap::new();
        let mut descriptors = Vec::with_capacity(batch.len());
        for (mut object, plaintext) in batch {
            if !matches!(object.status, CheckpointObjectStatus::Missing) {
                return Err(error::config(
                    "checkpoint physical batch has no upload owner",
                ));
            }
            let bytes = transfer::seal(self.workspace, &mut object, plaintext)?;
            descriptors.push(CheckpointUpload {
                object_id: object.object_id.clone(),
                ciphertext_sha256: afd_core::etag::sha256_hex(&bytes),
                ciphertext_length: bytes.len() as u64,
            });
            ciphertexts.insert(object.object_id.clone(), bytes);
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let response = self
            .plane
            .task_checkpoint_uploads(
                self.lease,
                &checkpoint,
                &CheckpointUploadsInput {
                    fencing_token: self.fence,
                    task: self.task.clone(),
                    pin_id: self.plan.pin.id.clone(),
                    objects: descriptors.clone(),
                },
            )
            .await?;
        let objects: Vec<Object> = response.items.into_iter().map(Object::from).collect();
        if objects.len() != descriptors.len() {
            return Err(error::config("checkpoint upload batch is incomplete"));
        }
        let mut sends = Vec::new();
        for authorized in objects {
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.object_id == authorized.object_id)
                .ok_or_else(|| error::config("checkpoint upload names another object"))?;
            if authorized.repository != self.plan.repository
                || authorized.ciphertext_sha256.as_deref()
                    != Some(descriptor.ciphertext_sha256.as_str())
                || authorized.ciphertext_length != Some(descriptor.ciphertext_length)
            {
                return Err(error::config("checkpoint ciphertext authorization differs"));
            }
            let bytes = ciphertexts
                .remove(&authorized.object_id)
                .ok_or_else(|| error::config("checkpoint upload identity repeated"))?;
            if !matches!(authorized.status, CheckpointObjectStatus::Reused) {
                sends.push((authorized, bytes));
            }
        }
        let transfers = &self.transfers;
        let outcomes = stream::iter(sends.into_iter().map(|(object, bytes)| async move {
            let transfer = object
                .transfer
                .as_ref()
                .ok_or_else(|| error::config("checkpoint upload transfer is absent"))?;
            transfers.upload(transfer, bytes).await
        }))
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
        self.verify_uploads(&descriptors, outcomes).await
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
                self.lease,
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
            let confirmed = objects.iter().any(|object| {
                object.object_id == descriptor.object_id
                    && object.repository == self.plan.repository
                    && object.ciphertext_sha256.as_deref()
                        == Some(descriptor.ciphertext_sha256.as_str())
                    && object.ciphertext_length == Some(descriptor.ciphertext_length)
                    && matches!(object.status, CheckpointObjectStatus::Reused)
            });
            if confirmed {
                self.progress.acknowledge(&descriptor.object_id);
            }
            complete &= confirmed;
        }
        self.progress(
            afd_wire::activity::TaskActivityOperation::Capture,
            afd_wire::activity::TaskActivityPhase::Uploading,
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

    /// Authorize at most four unique objects once, then restore concurrently.
    /// The result preserves manifest order, including repeated content chunks.
    pub(super) async fn download_batch(
        &mut self,
        ids: &[String],
    ) -> Result<Vec<afd_crypto::secret::SecretBytes>> {
        if ids.is_empty() || ids.len() > 4 {
            return Err(error::config("checkpoint restore batch exceeds its bound"));
        }
        self.renew().await?;
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let unique: std::collections::BTreeSet<_> = ids.iter().cloned().collect();
        let response = self
            .plane
            .task_checkpoint_authorizations(
                self.lease,
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
                    (17..=super::files::CHUNK + afd_crypto::TAG_LEN as u64).contains(length)
                })
                .ok_or_else(|| error::config("checkpoint ciphertext length is invalid"))?;
            self.progress.discover(&object.object_id, length, false);
        }
        self.progress(
            afd_wire::activity::TaskActivityOperation::Restore,
            afd_wire::activity::TaskActivityPhase::Restoring,
            None,
        )
        .await;
        let transfers = &self.transfers;
        let workspace = self.workspace;
        let outcomes = stream::iter(objects.into_iter().map(|mut object| async move {
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
                    restored.insert(id, bytes);
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
        self.progress(
            afd_wire::activity::TaskActivityOperation::Restore,
            afd_wire::activity::TaskActivityPhase::Restoring,
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
