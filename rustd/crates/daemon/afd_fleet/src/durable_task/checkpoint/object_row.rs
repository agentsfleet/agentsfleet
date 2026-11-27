//! Object rows become wire descriptors only after native scope and pin checks.

use super::row::Plan;
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal,
        row::counter,
        storage::{R2CheckpointStore, WorkspaceKey, recorded},
    },
};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext};
use base64::{Engine as _, prelude::BASE64_STANDARD};

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Object {
    pub id: Uuid7,
    pub content_identity: String,
    pub plaintext_digest: String,
    pub plaintext_size: i64,
    pub kind: String,
    pub format_version: i32,
    pub owner_checkpoint_id: Uuid7,
    pub owner_run_id: Uuid7,
    pub owner_fence: i64,
    pub bucket_reference: String,
    /// The cipher this object was registered under, as recorded.
    pub cipher: String,
    pub ciphertext_digest: Option<String>,
    pub ciphertext_length: Option<i64>,
    pub accounted_bytes: i64,
    pub verified: bool,
    pub tombstoned: bool,
    #[sqlx(rename = "object_nonce")]
    pub nonce: Vec<u8>,
}

impl Object {
    pub(super) fn own(&self, plan: &Plan) -> bool {
        self.belongs(&plan.id, &plan.owner_run_id, plan.owner_fence)
    }

    pub(crate) fn belongs(&self, checkpoint: &Uuid7, run: &Uuid7, fence: i64) -> bool {
        self.owner_checkpoint_id == *checkpoint
            && self.owner_run_id == *run
            && self.owner_fence == fence
    }

    /// Whether `key` is the key this object's identity was computed under.
    pub(crate) fn sealed_under(&self, key: &WorkspaceKey) -> bool {
        u64::try_from(self.plaintext_size).is_ok_and(|size| {
            let descriptor = CheckpointPlaintext {
                sha256: self.plaintext_digest.clone(),
                size,
                kind: self.kind.clone(),
            };
            key.content(&descriptor, self.format_version) == self.content_identity
        })
    }

    /// The object as a transfer names it, without the key.
    pub(crate) fn described(
        self,
        storage: &R2CheckpointStore,
        namespace: &str,
    ) -> Result<CheckpointObject> {
        if self.tombstoned || self.bucket_reference != storage.bucket_reference() {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let nonce: [u8; af_crypto::NONCE_LEN] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_invalid| DurableTaskRefusal::CheckpointIncompatible)?;
        Ok(CheckpointObject {
            object_id: self.id.into(),
            repository: namespace.to_owned(),
            plaintext: CheckpointPlaintext {
                sha256: self.plaintext_digest,
                size: counter(self.plaintext_size, "object plaintext_size")?,
                kind: self.kind,
            },
            status: if self.verified {
                CheckpointObjectStatus::Reused
            } else {
                CheckpointObjectStatus::Missing
            },
            cipher: recorded(&self.cipher)?,
            nonce: Some(BASE64_STANDARD.encode(nonce)),
            data_key: None,
            ciphertext_sha256: self.ciphertext_digest,
            ciphertext_length: self
                .ciphertext_length
                .map(|length| counter(length, "object ciphertext_length"))
                .transpose()?,
            transfer: None,
        })
    }

    pub(super) fn wire(
        self,
        storage: &R2CheckpointStore,
        plan: &Plan,
        key: &WorkspaceKey,
    ) -> Result<CheckpointObject> {
        self.wire_scoped(
            storage,
            &plan.repository,
            (&plan.id, &plan.owner_run_id, plan.owner_fence),
            key,
        )
    }

    /// The object as its registration answers: another owner's unfinished
    /// upload is busy and carries no key.
    pub(crate) fn wire_scoped(
        self,
        storage: &R2CheckpointStore,
        namespace: &str,
        owner: (&Uuid7, &Uuid7, i64),
        key: &WorkspaceKey,
    ) -> Result<CheckpointObject> {
        if !self.belongs(owner.0, owner.1, owner.2) && !self.verified {
            return Ok(CheckpointObject {
                object_id: self.id.into(),
                repository: namespace.to_owned(),
                plaintext: CheckpointPlaintext {
                    sha256: self.plaintext_digest,
                    size: counter(self.plaintext_size, "object plaintext_size")?,
                    kind: self.kind,
                },
                status: CheckpointObjectStatus::Busy,
                cipher: recorded(&self.cipher)?,
                nonce: None,
                data_key: None,
                ciphertext_sha256: None,
                ciphertext_length: None,
                transfer: None,
            });
        }
        let mut object = self.described(storage, namespace)?;
        object.data_key = Some(key.wire());
        Ok(object)
    }
}
