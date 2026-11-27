//! Shared immutable-object registration; ownership is supplied by the task or event transaction.

use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal,
        storage::{CheckpointKeys, chunk_cipher},
    },
    run::Runs,
};
use af_core::clock;
use af_wire::durable_task::{CheckpointCipher, CheckpointPlaintext};
use serde::Serialize;

#[derive(Clone, Copy)]
pub(crate) struct Registration<'a> {
    pub(crate) namespace: &'a str,
    pub(crate) format: i32,
    /// The cipher the owning plan recorded: its objects are sealed under it.
    pub(crate) cipher: CheckpointCipher,
}

#[derive(Serialize)]
pub(crate) struct NewObject {
    id: af_core::id::Uuid7,
    repository: String,
    content_identity: String,
    plaintext_digest: String,
    plaintext_size: i64,
    kind: String,
    format_version: i32,
    cipher: &'static str,
    pub(crate) accounted_bytes: i64,
    object_nonce: String,
}

/// One object row: its identity under the workspace's key, and a random nonce
/// of its own, because every object of the workspace is sealed under that key.
pub(crate) fn prepare(
    runs: &Runs,
    keys: &CheckpointKeys,
    scope: Registration<'_>,
    descriptor: &CheckpointPlaintext,
    content: &str,
) -> Result<NewObject> {
    let nonce = keys.nonce()?;
    let (plaintext_size, accounted_bytes) = sizes(descriptor.size, scope.cipher)?;
    Ok(NewObject {
        id: runs.entropy().uuid7(clock::now())?,
        repository: scope.namespace.to_owned(),
        content_identity: content.to_owned(),
        plaintext_digest: descriptor.sha256.clone(),
        plaintext_size,
        kind: descriptor.kind.clone(),
        format_version: scope.format,
        cipher: scope.cipher.into(),
        accounted_bytes,
        object_nonce: format!("\\x{}", hex::encode(nonce)),
    })
}

/// A plaintext of `size` bytes as stored, and the sealed bytes its upload
/// allowance is charged: the plaintext and whatever its cipher adds. Either
/// past a `BIGINT` exhausts the limit.
fn sizes(size: u64, cipher: CheckpointCipher) -> Result<(i64, i64)> {
    let plaintext = i64::try_from(size).map_err(|_overflow| DurableTaskRefusal::LimitExhausted)?;
    let accounted = i64::try_from(chunk_cipher(cipher).overhead())
        .ok()
        .and_then(|added| plaintext.checked_add(added))
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
    Ok((plaintext, accounted))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test: a size the helper should answer fails the test loudly"
)]
mod tests {
    use af_core::error_code;
    use af_wire::durable_task::CheckpointCipher;

    use super::sizes;

    // Value: protects=an object's stored and charged sizes are the plaintext and
    // the plaintext plus what its cipher adds; fails_when=the tag is dropped from
    // an encrypted charge, charged to a plain object, or added to the stored
    // size; why_new=the size pair had no direct case; seam=the private size
    // helper registration calls; no_claim=sealing.
    #[test]
    fn an_object_is_charged_its_plaintext_and_what_its_cipher_adds() {
        let tag = i64::try_from(af_crypto::TAG_LEN).expect("the tag is sixteen bytes");
        let sealed = sizes(5, CheckpointCipher::Aes256Gcm).expect("a small object fits");
        assert_eq!(sealed, (5, 5 + tag));
        let plain = sizes(5, CheckpointCipher::Plain).expect("a small object fits");
        assert_eq!(plain, (5, 5));
    }

    // Value: protects=a size the ledger cannot hold exhausts the limit instead of
    // wrapping or panicking; fails_when=either conversion stops refusing, or the
    // tag addition overflows unchecked; why_new=both refusals had no case;
    // seam=the private size helper registration calls; no_claim=garde's bound.
    #[test]
    fn a_size_past_a_bigint_exhausts_the_limit() {
        let largest = i64::MAX.unsigned_abs();
        for size in [u64::MAX, largest + 1, largest] {
            let refused = sizes(size, CheckpointCipher::Aes256Gcm)
                .expect_err("no BIGINT holds the sealed size");
            assert_eq!(
                refused.code(),
                error_code::DURABLE_TASK_LIMIT_EXHAUSTED,
                "{size}"
            );
        }
        sizes(largest, CheckpointCipher::Plain).expect("a plain object adds nothing");
    }
}
