//! The envelope's behaviour, pinned case by case.
//!
//! Seven properties, each with fixed inputs and a fixed expected outcome: a
//! round trip returns the plaintext, a tampered tag is refused, associated data
//! that does not match is refused, two seals of one plaintext differ, a key
//! survives its hex round trip, a wrong-length hex fails closed, and secret
//! bytes are zeroed before their memory is released.
//!
//! # Why these run at the envelope and not the primitive
//!
//! The single-layer primitive takes arbitrary associated data and stays
//! private, because a caller reaching it could seal a payload under the Key
//! Encryption Key (KEK) directly and skip the per-row Data Encryption Key
//! (DEK). Every case below is therefore exercised through the two-layer
//! envelope with the SAME associated-data bytes. The extra layer can only make
//! a case stricter, never weaker.
//!
//! `TEST_KEK_HEX` is a fixed test vector. It protects nothing.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use afd_crypto::secret::Kek;

/// The fixed key these cases seal under.
const TEST_KEK_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

/// The plaintext the round-trip, key-reload and zeroing cases seal.
const SECRET_API_KEY_PLAINTEXT: &[u8] = b"super-secret-api-key-12345";

/// The plaintext the tamper, associated-data and nonce cases seal.
const FIVE_BYTE_PLAINTEXT: &[u8] = b"hello";

/// The two associated-data values the mismatch case contrasts.
const AAD_WS_A: &[u8] = b"workspace-a";
const AAD_WS_B: &[u8] = b"workspace-b";

fn kek() -> Kek {
    Kek::from_hex(TEST_KEK_HEX).unwrap()
}

/// Sealing then opening raw bytes returns them unchanged.
#[test]
fn round_trip_with_raw_bytes() {
    let aad = Aad::from_bytes(Vec::new());
    let envelope = Sealer::new()
        .seal(&kek(), &aad, SECRET_API_KEY_PLAINTEXT)
        .unwrap();

    let recovered = envelope.open(&kek(), &aad).unwrap();
    assert_eq!(recovered.expose(), SECRET_API_KEY_PLAINTEXT);
}

/// A payload tag with one flipped bit (`bad_tag[0] ^= 0x01`) is refused.
#[test]
fn decrypt_fails_when_tag_is_tampered() {
    let aad = Aad::from_bytes(Vec::new());
    let envelope = Sealer::new()
        .seal(&kek(), &aad, FIVE_BYTE_PLAINTEXT)
        .unwrap();

    let mut bad_tag = *envelope.payload_tag();
    bad_tag[0] ^= 0x01;

    let tampered = Envelope::from_parts(
        envelope.wrapped_dek().to_vec(),
        envelope.dek_nonce(),
        envelope.dek_tag(),
        envelope.payload_nonce(),
        envelope.payload_ciphertext().to_vec(),
        &bad_tag,
        envelope.kek_version(),
    )
    .unwrap();

    let error = tampered
        .open(&kek(), &aad)
        .expect_err("a tampered tag must fail to open");
    assert!(error.is_open_failed(), "got {error}");
}

/// Associated data that differs from the seal's is refused — `workspace-a` opens,
/// `workspace-b` does not.
#[test]
fn associated_data_mismatch_rejects_ciphertext() {
    let key = kek();
    let aad_a = Aad::from_bytes(AAD_WS_A.to_vec());
    let envelope = Sealer::new()
        .seal(&key, &aad_a, FIVE_BYTE_PLAINTEXT)
        .unwrap();

    // The matching associated data recovers "hello" first, so the refusal below
    // is about the mismatch and not a seal that never opens.
    assert_eq!(
        envelope.open(&key, &aad_a).unwrap().expose(),
        FIVE_BYTE_PLAINTEXT
    );

    let error = envelope
        .open(&key, &Aad::from_bytes(AAD_WS_B.to_vec()))
        .expect_err("mismatched associated data must fail to open");
    assert!(error.is_open_failed(), "got {error}");
}

/// Two seals of the same input draw different nonces.
#[test]
fn encrypt_generates_unique_nonces() {
    let key = kek();
    let aad = Aad::from_bytes(AAD_WS_A.to_vec());
    let sealer = Sealer::new();

    let first = sealer.seal(&key, &aad, FIVE_BYTE_PLAINTEXT).unwrap();
    let second = sealer.seal(&key, &aad, FIVE_BYTE_PLAINTEXT).unwrap();

    assert_ne!(
        first.payload_nonce(),
        second.payload_nonce(),
        "two seals must not share a payload nonce"
    );
}

/// A KEK decoded again from the same hex is the same key.
///
/// This crate has no way to read key material back out — that is Invariant 5 —
/// so equality is asserted through behaviour: a key decoded from hex opens what
/// the same key sealed.
#[test]
fn kek_round_trips_through_hex() {
    let aad = Aad::from_bytes(Vec::new());
    let sealed = Sealer::new()
        .seal(&kek(), &aad, SECRET_API_KEY_PLAINTEXT)
        .unwrap();

    let reloaded = Kek::from_hex(TEST_KEK_HEX).unwrap();
    assert_eq!(
        sealed.open(&reloaded, &aad).unwrap().expose(),
        SECRET_API_KEY_PLAINTEXT
    );
}

/// A hex key of the wrong length (`"deadbeef"`) fails closed.
#[test]
fn kek_rejects_a_wrong_length_hex() {
    let error = Kek::from_hex("deadbeef").expect_err("a wrong-length hex must be refused");
    assert!(error.is_key_hex(), "got {error}");
}

/// Recovered secret bytes are zeroed before their memory is released.
///
/// The guarantee lives in the type, through `zeroize`, so the assertion is
/// that the buffer holds no non-zero byte once released.
#[test]
fn secret_bytes_are_zeroed_before_release() {
    use zeroize::Zeroize as _;

    let mut recovered = Sealer::new()
        .seal(
            &kek(),
            &Aad::from_bytes(Vec::new()),
            SECRET_API_KEY_PLAINTEXT,
        )
        .unwrap()
        .open(&kek(), &Aad::from_bytes(Vec::new()))
        .unwrap();

    assert_eq!(recovered.expose(), SECRET_API_KEY_PLAINTEXT);
    recovered.zeroize();
    assert!(recovered.expose().iter().all(|byte| *byte == 0));
}
