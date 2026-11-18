//! Ordinary and task checkpoints use the same bounded ciphertext staging and PUTs.

use super::{
    Object,
    catalog::Chunk,
    transfer::{self, Transfers},
};
use crate::{Result, error};
use af_core::id::Uuid7;
use af_wire::task::{CHECKPOINT_BATCH, CheckpointObjectStatus, CheckpointUpload};
use futures_util::{StreamExt as _, stream};
use std::collections::BTreeMap;

const CONCURRENT_PUTS: usize = 4;
pub(super) const BATCH_BYTES: u64 =
    CONCURRENT_PUTS as u64 * (super::files::CHUNK + af_crypto::TAG_LEN as u64);

pub(super) fn ciphertext_size(object: &Object) -> u64 {
    object.plaintext.size + crate::byte_count(af_crypto::TAG_LEN)
}

pub(super) struct UploadBatch {
    pub(super) descriptors: Vec<CheckpointUpload>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl UploadBatch {
    pub(super) fn seal(workspace: &Uuid7, pending: Vec<(Object, Chunk)>) -> Result<Self> {
        let bytes = pending.iter().fold(0_u64, |sum, (object, _)| {
            sum.saturating_add(ciphertext_size(object))
        });
        if pending.is_empty() || pending.len() > CHECKPOINT_BATCH || bytes > BATCH_BYTES {
            return Err(error::config("checkpoint physical batch exceeds its bound"));
        }
        let mut ciphertexts = BTreeMap::new();
        let mut descriptors = Vec::with_capacity(pending.len());
        for (mut object, chunk) in pending {
            if !matches!(object.status, CheckpointObjectStatus::Missing) {
                return Err(error::config(
                    "checkpoint physical batch has no upload owner",
                ));
            }
            let bytes = transfer::seal(workspace, &mut object, chunk.read()?)?;
            descriptors.push(CheckpointUpload {
                object_id: object.object_id.clone(),
                ciphertext_sha256: af_core::etag::sha256_hex(&bytes),
                ciphertext_length: crate::byte_count(bytes.len()),
            });
            if ciphertexts
                .insert(object.object_id.clone(), bytes)
                .is_some()
            {
                return Err(error::config("checkpoint upload identity repeated"));
            }
        }
        Ok(Self {
            descriptors,
            bytes: ciphertexts,
        })
    }

    pub(super) async fn put(
        &mut self,
        objects: Vec<af_wire::task::CheckpointObject>,
        namespace: &str,
        transfers: &Transfers,
    ) -> Result<Vec<Result<()>>> {
        let objects: Vec<_> = objects.into_iter().map(Object::from).collect();
        if objects.len() != self.descriptors.len() {
            return Err(error::config("checkpoint upload batch is incomplete"));
        }
        let mut sends = Vec::new();
        for object in objects {
            let descriptor = self
                .descriptors
                .iter()
                .find(|descriptor| descriptor.object_id == object.object_id)
                .ok_or_else(|| error::config("checkpoint upload names another object"))?;
            if !matches_descriptor(&object, descriptor, namespace) {
                return Err(error::config("checkpoint ciphertext authorization differs"));
            }
            let bytes = self
                .bytes
                .remove(&object.object_id)
                .ok_or_else(|| error::config("checkpoint upload identity repeated"))?;
            if !matches!(object.status, CheckpointObjectStatus::Reused) {
                sends.push((object, bytes));
            }
        }
        Ok(
            stream::iter(sends.into_iter().map(async move |(object, bytes)| {
                let transfer = object
                    .transfer
                    .as_ref()
                    .ok_or_else(|| error::config("checkpoint upload transfer is absent"))?;
                transfers.upload(transfer, bytes).await
            }))
            .buffer_unordered(CONCURRENT_PUTS)
            .collect()
            .await,
        )
    }
}

fn matches_descriptor(object: &Object, descriptor: &CheckpointUpload, namespace: &str) -> bool {
    object.object_id == descriptor.object_id
        && object.repository == namespace
        && object.ciphertext_sha256.as_deref() == Some(descriptor.ciphertext_sha256.as_str())
        && object.ciphertext_length == Some(descriptor.ciphertext_length)
}

pub(super) fn verified(object: &Object, descriptor: &CheckpointUpload, namespace: &str) -> bool {
    matches!(object.status, CheckpointObjectStatus::Reused)
        && matches_descriptor(object, descriptor, namespace)
}
