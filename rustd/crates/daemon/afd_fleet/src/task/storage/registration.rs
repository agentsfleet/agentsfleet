//! Shared immutable-object key custody; ownership is supplied by the task or event transaction.

use crate::{
    Result,
    lease::Leases,
    task::{TaskRefusal, storage::R2CheckpointStore},
};
use af_core::{clock, id::Uuid7};
use af_crypto::{
    checkpoint::{CheckpointScope, PreparedCheckpointKey},
    envelope::Sealer,
};
use af_wire::task::CheckpointPlaintext;
use serde::Serialize;

#[derive(Clone, Copy)]
pub(crate) struct Registration<'a> {
    pub(crate) workspace: &'a Uuid7,
    pub(crate) namespace: &'a str,
    pub(crate) identity: &'a Uuid7,
    pub(crate) format: i32,
}

#[derive(Serialize)]
pub(crate) struct NewObject {
    id: Uuid7,
    #[serde(rename = "task_identity_key_id")]
    identity_key_id: Uuid7,
    content_identity: String,
    plaintext_digest: String,
    plaintext_size: i64,
    kind: String,
    format_version: i32,
    pub(crate) accounted_bytes: i64,
    dek_ciphertext: String,
    dek_nonce: String,
    dek_tag: String,
    secret_nonce: String,
    secret_ciphertext: String,
    secret_tag: String,
    kek_version: i32,
    object_nonce: String,
}

pub(crate) fn prepare(
    leases: &Leases,
    storage: &R2CheckpointStore,
    scope: Registration<'_>,
    descriptor: &CheckpointPlaintext,
    content: &str,
) -> Result<NewObject> {
    let Registration {
        workspace,
        namespace,
        identity,
        format,
    } = scope;
    let id = leases.entropy().uuid7(clock::now())?;
    let prepared = PreparedCheckpointKey::create(
        leases.entropy(),
        &Sealer::new(),
        &storage.master,
        CheckpointScope {
            workspace_id: workspace,
            repository: namespace,
            object_id: &id,
            format_version: u32::try_from(format)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        },
    )?;
    let (wrapped, key) = prepared.into_parts();
    drop(key);
    let envelope = wrapped.envelope();
    Ok(NewObject {
        id,
        identity_key_id: identity.clone(),
        content_identity: content.to_owned(),
        plaintext_digest: descriptor.sha256.clone(),
        plaintext_size: i64::try_from(descriptor.size)
            .map_err(|_overflow| TaskRefusal::LimitExhausted)?,
        kind: descriptor.kind.clone(),
        format_version: format,
        accounted_bytes: accounted_bytes(descriptor.size)?,
        dek_ciphertext: pg_bytes(envelope.wrapped_dek()),
        dek_nonce: pg_bytes(envelope.dek_nonce()),
        dek_tag: pg_bytes(envelope.dek_tag()),
        secret_nonce: pg_bytes(envelope.payload_nonce()),
        secret_ciphertext: pg_bytes(envelope.payload_ciphertext()),
        secret_tag: pg_bytes(envelope.payload_tag()),
        kek_version: envelope.kek_version(),
        object_nonce: pg_bytes(wrapped.nonce()),
    })
}

fn accounted_bytes(size: u64) -> Result<i64> {
    Ok(i64::try_from(
        size + u64::try_from(af_crypto::TAG_LEN)
            .map_err(|_overflow| TaskRefusal::LimitExhausted)?,
    )
    .map_err(|_overflow| TaskRefusal::LimitExhausted)?)
}

fn pg_bytes(bytes: &[u8]) -> String {
    format!("\\x{}", hex::encode(bytes))
}
