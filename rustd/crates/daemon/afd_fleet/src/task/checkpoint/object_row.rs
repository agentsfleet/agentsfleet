//! Wrapped object material is opened only after native scope and pin checks.

use super::row::Plan;
use crate::{
    Result,
    task::{TaskRefusal, storage::R2CheckpointStore},
};
use af_core::id::Uuid7;
use af_crypto::{
    checkpoint::{CheckpointScope, WrappedCheckpointKey},
    secret::SecretBytes,
};
use af_wire::task::{CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext};
use afd_vault::StoredEnvelope;
use base64::{Engine as _, prelude::BASE64_STANDARD};

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Object {
    pub id: Uuid7,
    #[sqlx(rename = "task_identity_key_id")]
    pub identity_key_id: Uuid7,
    pub content_identity: String,
    pub plaintext_digest: String,
    pub plaintext_size: i64,
    pub kind: String,
    pub format_version: i32,
    pub owner_checkpoint_id: Uuid7,
    pub owner_lease_id: Uuid7,
    pub owner_fence: i64,
    pub bucket_reference: String,
    pub ciphertext_digest: Option<String>,
    pub ciphertext_length: Option<i64>,
    pub accounted_bytes: i64,
    pub verified: bool,
    pub tombstoned: bool,
    #[sqlx(flatten)]
    pub envelope: StoredEnvelope,
    #[sqlx(rename = "object_nonce")]
    pub nonce: Vec<u8>,
}

impl Object {
    pub(super) fn own(&self, plan: &Plan) -> bool {
        self.belongs(&plan.id, &plan.owner_lease_id, plan.owner_fence)
    }

    pub(crate) fn belongs(&self, checkpoint: &Uuid7, lease: &Uuid7, fence: i64) -> bool {
        self.owner_checkpoint_id == *checkpoint
            && self.owner_lease_id == *lease
            && self.owner_fence == fence
    }

    pub(super) fn open(
        self,
        storage: &R2CheckpointStore,
        plan: &Plan,
    ) -> Result<(CheckpointObject, SecretBytes)> {
        self.open_scoped(storage, &plan.workspace_id, &plan.repository)
    }

    pub(crate) fn open_scoped(
        self,
        storage: &R2CheckpointStore,
        workspace: &Uuid7,
        namespace: &str,
    ) -> Result<(CheckpointObject, SecretBytes)> {
        if self.tombstoned || self.bucket_reference != storage.bucket_reference() {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let id = self.id.clone();
        let nonce: [u8; af_crypto::NONCE_LEN] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_invalid| TaskRefusal::CheckpointIncompatible)?;
        let wrapped = WrappedCheckpointKey::from_parts(self.envelope.into_envelope()?, nonce);
        let key = wrapped.open(
            &storage.master,
            CheckpointScope {
                workspace_id: workspace,
                repository: namespace,
                object_id: &id,
                format_version: u32::try_from(self.format_version)
                    .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
            },
        )?;
        Ok((
            CheckpointObject {
                object_id: self.id.into(),
                repository: namespace.to_owned(),
                plaintext: CheckpointPlaintext {
                    sha256: self.plaintext_digest,
                    size: u64::try_from(self.plaintext_size)
                        .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
                    kind: self.kind,
                },
                status: if self.verified {
                    CheckpointObjectStatus::Reused
                } else {
                    CheckpointObjectStatus::Missing
                },
                nonce: Some(BASE64_STANDARD.encode(nonce)),
                data_key: None,
                ciphertext_sha256: self.ciphertext_digest,
                ciphertext_length: self
                    .ciphertext_length
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
                transfer: None,
            },
            key,
        ))
    }

    pub(super) fn wire(self, storage: &R2CheckpointStore, plan: &Plan) -> Result<CheckpointObject> {
        self.wire_scoped(
            storage,
            &plan.workspace_id,
            &plan.repository,
            (&plan.id, &plan.owner_lease_id, plan.owner_fence),
        )
    }

    pub(crate) fn wire_scoped(
        self,
        storage: &R2CheckpointStore,
        workspace: &Uuid7,
        namespace: &str,
        owner: (&Uuid7, &Uuid7, i64),
    ) -> Result<CheckpointObject> {
        if !self.belongs(owner.0, owner.1, owner.2) && !self.verified {
            return Ok(CheckpointObject {
                object_id: self.id.into(),
                repository: namespace.to_owned(),
                plaintext: CheckpointPlaintext {
                    sha256: self.plaintext_digest,
                    size: u64::try_from(self.plaintext_size)
                        .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
                    kind: self.kind,
                },
                status: CheckpointObjectStatus::Busy,
                nonce: None,
                data_key: None,
                ciphertext_sha256: None,
                ciphertext_length: None,
                transfer: None,
            });
        }
        let (mut object, key) = self.open_scoped(storage, workspace, namespace)?;
        object.data_key = Some(BASE64_STANDARD.encode(key.expose()));
        Ok(object)
    }
}
