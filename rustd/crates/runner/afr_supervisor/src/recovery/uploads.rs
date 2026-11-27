//! Ordinary and task checkpoints register their chunks, then seal and PUT only
//! the missing ones, in batches of bounded ciphertext.
//!
//! A file chunk's bytes are read a second time from the pin's changes rather
//! than held from the first read, so a save holds at most one batch of
//! plaintext. The seal checks each chunk against its registration, so bytes
//! that changed between the two reads are never uploaded.

use super::{Object, catalog::Catalog, transfer::Transfers, tree::ChangeSource};
use crate::{Result, error};
use af_chunk_store::{Change, ChunkKey};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    CHECKPOINT_BATCH, CheckpointObjectStatus, CheckpointPlaintext, CheckpointUpload,
};
use futures_util::{StreamExt as _, stream};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

const CONCURRENT_PUTS: usize = 4;
pub(super) const BATCH_BYTES: u64 =
    CONCURRENT_PUTS as u64 * (super::files::CHUNK + af_crypto::TAG_LEN as u64);

pub(super) fn ciphertext_size(object: &af_wire::durable_task::CheckpointObject) -> u64 {
    object.plaintext.size + crate::byte_count(af_chunk_store::cipher(object.cipher).overhead())
}

/// Logs how one owner's publication ended: the generation it committed, or
/// the failure that left the previous generation current.
pub(super) fn logged<T>(
    owner_kind: &'static str,
    started: tokio::time::Instant,
    published: &Result<T>,
    generation: impl FnOnce(&T) -> u64,
) {
    match published {
        Ok(committed) => {
            let generation = generation(committed);
            let publish_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            tracing::info!(
                event = "checkpoint_published",
                owner_kind,
                generation,
                publish_ms
            );
        }
        Err(failure) => {
            let error_code = failure.code();
            tracing::warn!(event = "checkpoint_publication_failed", owner_kind, %error_code);
        }
    }
}

/// The daemon end of one save: register plaintext, then upload a sealed batch.
pub(super) trait Publisher {
    /// Registers `descriptors`, answering one object for each, in order.
    async fn register(&mut self, descriptors: Vec<CheckpointPlaintext>) -> Result<Vec<Object>>;

    /// Told each registered batch, out of `total` distinct chunks.
    async fn registered(&mut self, objects: &[Object], total: usize);

    /// Seals and PUTs `pending`; the objects whose PUT was accepted.
    async fn upload_batch(
        &mut self,
        pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> Result<Vec<String>>;
}

/// What a save registered and uploaded.
pub(super) struct Published {
    /// Each distinct chunk's object.
    pub(super) ids: BTreeMap<String, String>,
    /// The objects whose PUT the bucket accepted.
    pub(super) uploaded: Vec<String>,
}

/// Registers every chunk of `catalog`, then uploads the missing ones: history
/// from memory, file chunks from a second read of `source`.
///
/// # Errors
/// A registration or upload failed, a chunk has another live writer, or a
/// missing chunk is no longer in `source`.
pub(super) async fn publish(
    publisher: &mut impl Publisher,
    catalog: &Catalog,
    source: Option<&dyn ChangeSource>,
) -> Result<Published> {
    let (ids, mut missing) = register(publisher, catalog).await?;
    let mut pending = Pending::default();
    for (key, bytes) in catalog.history()? {
        if let Some(object) = missing.remove(key) {
            pending.push(publisher, object, bytes).await?;
        }
    }
    if !missing.is_empty() {
        let source = source.ok_or_else(|| error::config("checkpoint file has no workspace"))?;
        let mut changes = source.changes();
        while !missing.is_empty()
            && let Some(change) = changes.next().await
        {
            let Change::Chunk {
                key: ChunkKey::File { path, index },
                bytes,
            } = change?
            else {
                continue;
            };
            let object = catalog
                .file_chunk(&path, index)
                .and_then(|key| missing.remove(key));
            if let Some(object) = object {
                pending.push(publisher, object, bytes).await?;
            }
        }
    }
    if !missing.is_empty() {
        return Err(error::config("checkpoint chunk vanished before its upload"));
    }
    pending.flush(publisher).await?;
    Ok(Published {
        ids,
        uploaded: pending.uploaded,
    })
}

/// Each distinct chunk's object, and the objects still missing their bytes.
type Registered = (BTreeMap<String, String>, BTreeMap<String, Object>);

/// Registers every distinct chunk of `catalog`, in bounded batches: each
/// chunk's object, and the objects the daemon holds no bytes for yet.
///
/// # Errors
/// A registration failed, or a chunk has another live writer.
async fn register(publisher: &mut impl Publisher, catalog: &Catalog) -> Result<Registered> {
    let chunks: Vec<_> = catalog.chunks.iter().collect();
    let mut ids = BTreeMap::new();
    let mut missing = BTreeMap::new();
    for batch in chunks.chunks(CHECKPOINT_BATCH) {
        let descriptors = batch.iter().map(|(_, chunk)| chunk.descriptor.clone());
        let objects = publisher.register(descriptors.collect()).await?;
        publisher.registered(&objects, chunks.len()).await;
        for ((key, _), object) in batch.iter().zip(objects) {
            ids.insert((*key).clone(), object.object_id.clone());
            match object.status {
                CheckpointObjectStatus::Busy => {
                    return Err(error::capture_unavailable(
                        "checkpoint chunk has another live writer",
                    ));
                }
                CheckpointObjectStatus::Reused => {}
                CheckpointObjectStatus::Missing => {
                    missing.insert((*key).clone(), object);
                }
            }
        }
    }
    Ok((ids, missing))
}

/// Chunks waiting to be sealed and PUT together, and what was accepted.
#[derive(Default)]
struct Pending {
    batch: Vec<(Object, Zeroizing<Vec<u8>>)>,
    bytes: u64,
    uploaded: Vec<String>,
}

impl Pending {
    async fn push(
        &mut self,
        publisher: &mut impl Publisher,
        object: Object,
        bytes: Zeroizing<Vec<u8>>,
    ) -> Result<()> {
        let size = ciphertext_size(&object);
        if !self.batch.is_empty() && self.bytes + size > BATCH_BYTES {
            self.flush(publisher).await?;
        }
        self.batch.push((object, bytes));
        self.bytes += size;
        Ok(())
    }

    async fn flush(&mut self, publisher: &mut impl Publisher) -> Result<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        self.bytes = 0;
        let accepted = publisher
            .upload_batch(std::mem::take(&mut self.batch))
            .await?;
        self.uploaded.extend(accepted);
        Ok(())
    }
}

pub(super) struct UploadBatch {
    pub(super) descriptors: Vec<CheckpointUpload>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl UploadBatch {
    pub(super) fn seal(
        workspace: &Uuid7,
        pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> Result<Self> {
        let bytes = pending.iter().fold(0_u64, |sum, (object, _)| {
            sum.saturating_add(ciphertext_size(object))
        });
        if pending.is_empty() || pending.len() > CHECKPOINT_BATCH || bytes > BATCH_BYTES {
            return Err(error::config("checkpoint physical batch exceeds its bound"));
        }
        let mut ciphertexts = BTreeMap::new();
        let mut descriptors = Vec::with_capacity(pending.len());
        for (mut object, plaintext) in pending {
            if !matches!(object.status, CheckpointObjectStatus::Missing) {
                return Err(error::config(
                    "checkpoint physical batch has no upload owner",
                ));
            }
            let bytes = af_chunk_store::seal(workspace, &mut object, plaintext)?;
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

    /// PUTs each sealed chunk through the link the daemon answered for it,
    /// four at once; the objects the bucket accepted.
    ///
    /// # Errors
    /// A link is missing or names other bytes, or a PUT was not accepted.
    pub(super) async fn put(
        &mut self,
        objects: Vec<af_wire::durable_task::CheckpointObject>,
        namespace: &str,
        transfers: &Transfers,
    ) -> Result<Vec<String>> {
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
        let outcomes: Vec<Result<String>> =
            stream::iter(sends.into_iter().map(async move |(object, bytes)| {
                let transfer = object
                    .transfer
                    .as_ref()
                    .ok_or_else(|| error::config("checkpoint upload transfer is absent"))?;
                transfers.upload(transfer, bytes).await?;
                Ok(object.object_id.clone())
            }))
            .buffer_unordered(CONCURRENT_PUTS)
            .collect()
            .await;
        outcomes.into_iter().collect()
    }
}

fn matches_descriptor(object: &Object, descriptor: &CheckpointUpload, namespace: &str) -> bool {
    object.object_id == descriptor.object_id
        && object.repository == namespace
        && object.ciphertext_sha256.as_deref() == Some(descriptor.ciphertext_sha256.as_str())
        && object.ciphertext_length == Some(descriptor.ciphertext_length)
}
