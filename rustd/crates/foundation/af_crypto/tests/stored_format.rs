//! Stored ciphertext crosses the move from `RustCrypto` to AWS-LC in both directions.
//!
//! The bytes below were sealed by the `RustCrypto` build of this crate (`aes-gcm`
//! 0.11.1) before it was removed, and are checked in rather than regenerated:
//! no code left in the tree can produce them. Opening them proves every vault
//! row and checkpoint object written before the move opens after it. Sealing
//! the same inputs under pinned entropy and asserting the same bytes proves the
//! reverse: what this build writes, the earlier one opens.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher};
use af_crypto::envelope::{Envelope, Sealer};
use af_crypto::secret::{Kek, SecretBytes};
use af_crypto::{KEK_VERSION, KEY_LEN, NONCE_LEN};
use zeroize::Zeroizing;

const WORKSPACE: &str = "019f01d0-0000-7000-8000-000000000001";
const OBJECT: &str = "019f01d0-0000-7000-8000-000000000002";

const VAULT_PLAINTEXT: &[u8] = b"{\"api_key\":\"not-a-real-key\"}";
const KEK_BYTE: u8 = 0x11;
const DEK_BYTE: u8 = 0x22;
const DEK_NONCE: [u8; NONCE_LEN] = [0x33; NONCE_LEN];
const PAYLOAD_NONCE: [u8; NONCE_LEN] = [0x44; NONCE_LEN];
const WRAPPED_DEK: &str = "a6a17064e8b0570e909ac94f8509c10a24ea2d16a8fba78eda682ba781aca808";
const DEK_TAG: &str = "64e6efc60ecb295a57c34d47c2bf368e";
const PAYLOAD_CIPHERTEXT: &str = "e92dc4138b7c88e7cc8760024cb2a3ec70a48172abca901b5e270077";
const PAYLOAD_TAG: &str = "a2dd94da16bf9430d9cc8c0320f523d7";

const CHUNK_PLAINTEXT: &[u8] = b"checkpoint chunk sealed before aws-lc-rs";
const CHUNK_KEY_BYTE: u8 = 0x55;
const CHUNK_NONCE: [u8; NONCE_LEN] = [0x66; NONCE_LEN];
/// Ciphertext, then its 16-byte tag, as the bucket stores it.
const CHUNK_SEALED: &str = "5cd5db311e6c4620ae1d3066cfdd90090f470e335111c56805eef90b\
                            52aa5f75427750fc9a0ae8d41100dd09fd8b58430b595fa98c8f1386";

fn vault_aad() -> Aad {
    Aad::new(WORKSPACE, "openai")
}

fn kek() -> Kek {
    Kek::from_bytes([KEK_BYTE; KEY_LEN])
}

fn chunk_key() -> SecretBytes {
    SecretBytes::new(vec![CHUNK_KEY_BYTE; KEY_LEN])
}

fn unhex(text: &str) -> Vec<u8> {
    hex::decode(text.replace(char::is_whitespace, "")).unwrap()
}

#[test]
fn test_vault_row_sealed_by_rustcrypto_opens() {
    let envelope = Envelope::from_parts(
        unhex(WRAPPED_DEK),
        &DEK_NONCE,
        &unhex(DEK_TAG),
        &PAYLOAD_NONCE,
        unhex(PAYLOAD_CIPHERTEXT),
        &unhex(PAYLOAD_TAG),
        KEK_VERSION,
    )
    .unwrap();
    let opened = envelope.open(&kek(), &vault_aad()).unwrap();
    assert_eq!(opened.expose(), VAULT_PLAINTEXT);
}

#[test]
fn test_vault_row_seals_to_the_rustcrypto_bytes() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.push_bytes(&[DEK_BYTE; KEY_LEN]);
    entropy.push_bytes(&DEK_NONCE);
    entropy.push_bytes(&PAYLOAD_NONCE);
    let envelope = sealer.seal(&kek(), &vault_aad(), VAULT_PLAINTEXT).unwrap();
    assert_eq!(hex::encode(envelope.wrapped_dek()), WRAPPED_DEK);
    assert_eq!(hex::encode(envelope.dek_tag()), DEK_TAG);
    assert_eq!(
        hex::encode(envelope.payload_ciphertext()),
        PAYLOAD_CIPHERTEXT
    );
    assert_eq!(hex::encode(envelope.payload_tag()), PAYLOAD_TAG);
}

fn chunk_binding() -> Aad {
    let workspace = Uuid7::parse(WORKSPACE).unwrap();
    let object = Uuid7::parse(OBJECT).unwrap();
    CheckpointScope {
        workspace_id: &workspace,
        repository: "github:repo:123",
        object_id: &object,
        format_version: 1,
    }
    .binding()
}

#[test]
fn test_checkpoint_object_sealed_by_rustcrypto_opens() {
    let opened = Aes256Gcm
        .open(
            &chunk_key(),
            &CHUNK_NONCE,
            &chunk_binding(),
            unhex(CHUNK_SEALED),
        )
        .unwrap();
    assert_eq!(opened.expose(), CHUNK_PLAINTEXT);
}

#[test]
fn test_checkpoint_object_seals_to_the_rustcrypto_bytes() {
    let sealed = Aes256Gcm
        .seal(
            &chunk_key(),
            &CHUNK_NONCE,
            &chunk_binding(),
            Zeroizing::new(CHUNK_PLAINTEXT.to_vec()),
        )
        .unwrap();
    assert_eq!(sealed, unhex(CHUNK_SEALED));
}
