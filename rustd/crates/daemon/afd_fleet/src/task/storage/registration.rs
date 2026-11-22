//! Shared immutable-object key custody; ownership is supplied by the task or event transaction.

use crate::{
    Result,
    run::Runs,
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
    runs: &Runs,
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
    let id = runs.entropy().uuid7(clock::now())?;
    let prepared = PreparedCheckpointKey::create(
        runs.entropy(),
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
    let (plaintext_size, accounted_bytes) = sizes(descriptor.size)?;
    Ok(NewObject {
        id,
        identity_key_id: identity.clone(),
        content_identity: content.to_owned(),
        plaintext_digest: descriptor.sha256.clone(),
        plaintext_size,
        kind: descriptor.kind.clone(),
        format_version: format,
        accounted_bytes,
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

/// A plaintext of `size` bytes as stored, and the sealed bytes its upload
/// allowance is charged: the plaintext and its authentication tag. Either past
/// a `BIGINT` exhausts the limit.
fn sizes(size: u64) -> Result<(i64, i64)> {
    let plaintext = i64::try_from(size).map_err(|_overflow| TaskRefusal::LimitExhausted)?;
    let accounted = i64::try_from(af_crypto::TAG_LEN)
        .ok()
        .and_then(|tag| plaintext.checked_add(tag))
        .ok_or(TaskRefusal::LimitExhausted)?;
    Ok((plaintext, accounted))
}

fn pg_bytes(bytes: &[u8]) -> String {
    format!("\\x{}", hex::encode(bytes))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test: a size the helper should answer fails the test loudly"
)]
mod tests {
    use af_core::error_code;

    use super::sizes;

    // Value: protects=an object's stored and charged sizes are the plaintext and
    // the plaintext plus its tag; fails_when=the tag is dropped from the charge or
    // added to the stored size; why_new=the size pair had no direct case;
    // seam=the private size helper registration calls; no_claim=key sealing.
    #[test]
    fn an_object_is_charged_its_plaintext_and_its_tag() {
        let (plaintext, accounted) = sizes(5).expect("a small object fits");
        assert_eq!(plaintext, 5);
        assert_eq!(
            accounted,
            5 + i64::try_from(af_crypto::TAG_LEN).expect("the tag is sixteen bytes")
        );
    }

    // Value: protects=a size the ledger cannot hold exhausts the limit instead of
    // wrapping or panicking; fails_when=either conversion stops refusing, or the
    // tag addition overflows unchecked; why_new=both refusals had no case;
    // seam=the private size helper registration calls; no_claim=garde's bound.
    #[test]
    fn a_size_past_a_bigint_exhausts_the_limit() {
        let largest = i64::MAX.unsigned_abs();
        for size in [u64::MAX, largest + 1, largest] {
            let refused = sizes(size).expect_err("no BIGINT holds the sealed size");
            assert_eq!(refused.code(), error_code::TASK_LIMIT_EXHAUSTED, "{size}");
        }
    }
}
