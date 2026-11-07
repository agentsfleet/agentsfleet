//! Receipt bytes use the existing envelope encryption and a per-call local key.

use super::super::authoring::files;
use crate::{Result, error};
use af_crypto::{
    aad::Aad,
    entropy::Entropy,
    envelope::{Envelope, Sealer},
    secret::Kek,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

const KEY: &str = "receipt.key";
pub(super) const CONTENT: &str = "receipt.enc";
const MAX_BYTES: u64 = 8 * af_wire::decision::REPLY_MAX_BYTES as u64;

#[derive(Serialize, Deserialize)]
struct Ciphertext {
    wrapped_key: Vec<u8>,
    key_nonce: Vec<u8>,
    key_tag: Vec<u8>,
    nonce: Vec<u8>,
    bytes: Vec<u8>,
    tag: Vec<u8>,
}

pub(super) fn create(directory: &Path) -> Result<()> {
    let mut bytes = Zeroizing::new([0_u8; af_crypto::KEY_LEN]);
    Entropy::new().fill(bytes.as_mut_slice())?;
    files::write(&directory.join(KEY), bytes.as_slice())
}

fn key(directory: &Path) -> Result<Kek> {
    let bytes = Zeroizing::new(files::read(
        &directory.join(KEY),
        af_crypto::KEY_LEN as u64,
    )?);
    let key = bytes
        .as_slice()
        .try_into()
        .map_err(|_size| error::config("decision receipt key has an invalid size"))?;
    Ok(Kek::from_bytes(key))
}

pub(super) fn write(directory: &Path, call: &str, plaintext: &[u8]) -> Result<()> {
    let aad = Aad::new(call, "agentsfleet.decision-receipt.v1");
    let envelope = Sealer::new().seal(&key(directory)?, &aad, plaintext)?;
    let stored = Ciphertext {
        wrapped_key: envelope.wrapped_dek().to_vec(),
        key_nonce: envelope.dek_nonce().to_vec(),
        key_tag: envelope.dek_tag().to_vec(),
        nonce: envelope.payload_nonce().to_vec(),
        bytes: envelope.payload_ciphertext().to_vec(),
        tag: envelope.payload_tag().to_vec(),
    };
    files::write(
        &directory.join(CONTENT),
        &serde_json::to_vec(&stored).map_err(error::encode)?,
    )
}

pub(super) fn read(directory: &Path, call: &str) -> Result<af_crypto::secret::SecretBytes> {
    let stored: Ciphertext =
        serde_json::from_slice(&files::read(&directory.join(CONTENT), MAX_BYTES)?)
            .map_err(error::malformed(crate::client::Verb::TaskDecisionRecord))?;
    let envelope = Envelope::from_parts(
        stored.wrapped_key,
        &stored.key_nonce,
        &stored.key_tag,
        &stored.nonce,
        stored.bytes,
        &stored.tag,
        af_crypto::KEK_VERSION,
    )?;
    Ok(envelope.open(
        &key(directory)?,
        &Aad::new(call, "agentsfleet.decision-receipt.v1"),
    )?)
}
