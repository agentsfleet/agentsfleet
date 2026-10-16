//! Metadata authenticates before the encrypted private component is opened.

use super::{entry::Metadata, files};
use crate::{Result, error};
use afd_crypto::{
    aad::Aad,
    entropy::Entropy,
    envelope::{Envelope, Sealer},
    mac::HmacSha256Tag,
    secret::Kek,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

const COMPONENT_MAX: u64 = 1024 * 1024;
const META_MAX: u64 = 4096;

#[derive(Serialize, Deserialize)]
struct ProtectedMetadata {
    metadata: Metadata,
    authentication: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct Ciphertext {
    wrapped_key: Vec<u8>,
    key_nonce: Vec<u8>,
    key_tag: Vec<u8>,
    nonce: Vec<u8>,
    bytes: Vec<u8>,
    tag: Vec<u8>,
}

pub(super) fn seal(directory: &Path, metadata: Metadata, answer: &str) -> Result<()> {
    let mut key = Zeroizing::new([0_u8; afd_crypto::KEY_LEN]);
    Entropy::new().fill(key.as_mut())?;
    let kek = Kek::from_bytes(*key);
    let canonical = serde_json::to_vec(&metadata).map_err(error::encode)?;
    let aad = Aad::new(&metadata.attempt_id, &String::from_utf8_lossy(&canonical));
    let envelope = Sealer::new().seal(&kek, &aad, answer.as_bytes())?;
    let ciphertext = Ciphertext {
        wrapped_key: envelope.wrapped_dek().to_vec(),
        key_nonce: envelope.dek_nonce().to_vec(),
        key_tag: envelope.dek_tag().to_vec(),
        nonce: envelope.payload_nonce().to_vec(),
        bytes: envelope.payload_ciphertext().to_vec(),
        tag: envelope.payload_tag().to_vec(),
    };
    let protected = ProtectedMetadata {
        authentication: HmacSha256Tag::compute(&kek, &canonical).as_bytes().to_vec(),
        metadata,
    };
    files::write(&directory.join(files::KEY), key.as_slice())?;
    files::write(
        &directory.join(files::CONTENT),
        &serde_json::to_vec(&ciphertext).map_err(error::encode)?,
    )?;
    files::write(
        &directory.join(files::META),
        &serde_json::to_vec(&protected).map_err(error::encode)?,
    )
}

pub(super) fn metadata(directory: &Path) -> Result<(Metadata, Kek)> {
    let protected: ProtectedMetadata =
        serde_json::from_slice(&files::read(&directory.join(files::META), META_MAX)?)
            .map_err(error::malformed(crate::client::Verb::AuthoringReport))?;
    let bytes = Zeroizing::new(files::read(
        &directory.join(files::KEY),
        afd_crypto::KEY_LEN as u64,
    )?);
    let key = bytes
        .as_slice()
        .try_into()
        .map_err(|_wrong_length| error::config("a private report key has the wrong length"))?;
    let key = Kek::from_bytes(key);
    let canonical = serde_json::to_vec(&protected.metadata).map_err(error::encode)?;
    HmacSha256Tag::compute(&key, &canonical)
        .verify(&HmacSha256Tag::from_slice(&protected.authentication)?)?;
    Ok((protected.metadata, key))
}

pub(super) fn open(directory: &Path, metadata: &Metadata, key: &Kek) -> Result<String> {
    let ciphertext: Ciphertext = serde_json::from_slice(&files::read(
        &directory.join(files::CONTENT),
        COMPONENT_MAX,
    )?)
    .map_err(error::malformed(crate::client::Verb::AuthoringReport))?;
    let canonical = serde_json::to_string(metadata).map_err(error::encode)?;
    let aad = Aad::new(&metadata.attempt_id, &canonical);
    let envelope = Envelope::from_parts(
        ciphertext.wrapped_key,
        &ciphertext.key_nonce,
        &ciphertext.key_tag,
        &ciphertext.nonce,
        ciphertext.bytes,
        &ciphertext.tag,
        afd_crypto::KEK_VERSION,
    )?;
    let plaintext = envelope.open(key, &aad)?;
    let answer = std::str::from_utf8(plaintext.expose())
        .map_err(|_not_text| error::config("a private report answer is not UTF-8"))?;
    Ok(answer.to_owned())
}
