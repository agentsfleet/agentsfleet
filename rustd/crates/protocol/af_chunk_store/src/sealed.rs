//! A chunk sealed for the bucket, and the check a stored object passes before
//! a restore uses one byte of it.
//!
//! Each object is sealed and opened under the workspace's key the daemon hands
//! over with its authorization, by the cipher the daemon recorded for it. The
//! key rides the registration once: sealing or opening takes it out, so it is
//! never used twice from one handover.

use af_core::id::Uuid7;
use af_crypto::{
    checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher, Plain},
    secret::SecretBytes,
};
use af_wire::durable_task::{CHECKPOINT_OBJECT_BYTES, CheckpointCipher, CheckpointObject};
use base64::Engine as _;
use zeroize::Zeroizing;

use crate::error::{Result, encoding, refused};

/// The largest stored object: a full chunk and the tag the encrypting cipher
/// adds.
pub const MAX_OBJECT_BYTES: u64 = CHECKPOINT_OBJECT_BYTES + af_crypto::TAG_LEN as u64;

/// The manifest format every object is bound to.
const FORMAT_VERSION: u32 = 1;

/// The implementation the daemon recorded for an object: a save seals with it
/// and a restore opens with it, whatever the deployment's setting is now.
#[must_use]
pub fn cipher(named: CheckpointCipher) -> &'static dyn ChunkCipher {
    match named {
        CheckpointCipher::Aes256Gcm => &Aes256Gcm,
        CheckpointCipher::Plain => &Plain,
    }
}

/// An object's stored length, when it is one its cipher can have produced
/// from a chunk of at least one byte.
///
/// # Errors
/// The length is absent, or no chunk sealed under the object's cipher has it.
pub fn stored_length(object: &CheckpointObject) -> Result<u64> {
    let smallest = 1 + byte_count(cipher(object.cipher).overhead());
    object
        .ciphertext_length
        .filter(|length| (smallest..=MAX_OBJECT_BYTES).contains(length))
        .ok_or_else(|| refused("checkpoint ciphertext length is invalid"))
}

/// Seals `plaintext` for the bucket, once it is the plaintext `object`
/// registered.
///
/// # Errors
/// The plaintext is not the one registered, the key or nonce is absent or
/// malformed, or the cipher refused.
pub fn seal(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    plaintext: Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>> {
    if byte_count(plaintext.len()) != object.plaintext.size
        || af_core::etag::sha256_hex(&plaintext) != object.plaintext.sha256
    {
        return Err(refused(
            "checkpoint plaintext differs from native registration",
        ));
    }
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let scope = scope(workspace, &object.repository, &id);
    Ok(cipher(object.cipher).seal(&key, &nonce, &scope.binding(), plaintext)?)
}

/// The plaintext of a stored object, once it is the object registered.
///
/// Its ciphertext digest and length, its tag, and its plaintext digest and
/// size must all match: a restore writes nothing one check let through alone.
///
/// # Errors
/// Any one of those checks fails, or the key or nonce is absent or malformed.
pub fn open(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    ciphertext: Vec<u8>,
) -> Result<SecretBytes> {
    if object.ciphertext_length != Some(byte_count(ciphertext.len()))
        || byte_count(ciphertext.len()) > MAX_OBJECT_BYTES
        || object.ciphertext_sha256.as_deref()
            != Some(af_core::etag::sha256_hex(&ciphertext).as_str())
    {
        return Err(refused(
            "checkpoint ciphertext differs from native registration",
        ));
    }
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let scope = scope(workspace, &object.repository, &id);
    let plaintext = cipher(object.cipher).open(&key, &nonce, &scope.binding(), ciphertext)?;
    if byte_count(plaintext.expose().len()) != object.plaintext.size
        || af_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(refused("checkpoint plaintext authentication mismatch"));
    }
    Ok(plaintext)
}

/// What an object is bound to: a sealed chunk opens only as the object of the
/// repository and workspace it was sealed for.
const fn scope<'a>(
    workspace: &'a Uuid7,
    repository: &'a str,
    object: &'a Uuid7,
) -> CheckpointScope<'a> {
    CheckpointScope {
        workspace_id: workspace,
        repository,
        object_id: object,
        format_version: FORMAT_VERSION,
    }
}

/// The key and nonce the daemon handed over, the key taken out of `object`.
fn material(object: &mut CheckpointObject) -> Result<(SecretBytes, [u8; af_crypto::NONCE_LEN])> {
    let key = Zeroizing::new(
        object
            .data_key
            .take()
            .ok_or_else(|| refused("checkpoint key is absent"))?,
    );
    let key = SecretBytes::new(
        base64::prelude::BASE64_STANDARD
            .decode(key.as_bytes())
            .map_err(|source| encoding("checkpoint key encoding is invalid", source))?,
    );
    let nonce = base64::prelude::BASE64_STANDARD
        .decode(
            object
                .nonce
                .as_deref()
                .ok_or_else(|| refused("checkpoint nonce is absent"))?,
        )
        .map_err(|source| encoding("checkpoint nonce encoding is invalid", source))?;
    let nonce = nonce
        .as_slice()
        .try_into()
        .map_err(|_wrong| refused("checkpoint nonce length is invalid"))?;
    Ok((key, nonce))
}

/// A length in memory as the `u64` the wire counts bytes in.
fn byte_count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "sealed_tests.rs"]
mod tests;
