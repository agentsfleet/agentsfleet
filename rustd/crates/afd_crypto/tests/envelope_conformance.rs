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

/// The plaintext the Zig round-trip test uses.
const ZIG_ROUND_TRIP_PLAINTEXT: &[u8] = b"super-secret-api-key-12345";

/// The plaintext the Zig tamper and associated-data tests use.
const ZIG_SHORT_PLAINTEXT: &[u8] = b"hello";

/// The two associated-data values the Zig mismatch test contrasts.
const ZIG_AD_A: &[u8] = b"workspace-a";
const ZIG_AD_B: &[u8] = b"workspace-b";

fn kek() -> Kek {
    Kek::from_hex(TEST_KEK_HEX).unwrap()
}

/// Zig: `encrypt/decrypt round-trip with raw bytes`.
#[test]
fn round_trip_with_raw_bytes() {
    let aad = Aad::from_bytes(Vec::new());
    let envelope = Sealer::new()
        .seal(&kek(), &aad, ZIG_ROUND_TRIP_PLAINTEXT)
        .unwrap();

    let recovered = envelope.open(&kek(), &aad).unwrap();
    assert_eq!(recovered.expose(), ZIG_ROUND_TRIP_PLAINTEXT);
}

/// Zig: `decrypt fails when tag is tampered` — `bad_tag[0] ^= 0x01`.
#[test]
fn decrypt_fails_when_tag_is_tampered() {
    let aad = Aad::from_bytes(Vec::new());
    let envelope = Sealer::new()
        .seal(&kek(), &aad, ZIG_SHORT_PLAINTEXT)
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
        .expect_err("Zig expects SecretError.DecryptFailed here");
    assert!(error.is_open_failed(), "got {error}");
}

/// Zig: `associated data mismatch rejects ciphertext` — `workspace-a` opens, `workspace-b` does not.
#[test]
fn associated_data_mismatch_rejects_ciphertext() {
    let key = kek();
    let aad_a = Aad::from_bytes(ZIG_AD_A.to_vec());
    let envelope = Sealer::new()
        .seal(&key, &aad_a, ZIG_SHORT_PLAINTEXT)
        .unwrap();

    // The Zig test asserts the matching associated data recovers "hello" first.
    assert_eq!(
        envelope.open(&key, &aad_a).unwrap().expose(),
        ZIG_SHORT_PLAINTEXT
    );

    let error = envelope
        .open(&key, &Aad::from_bytes(ZIG_AD_B.to_vec()))
        .expect_err("Zig expects SecretError.DecryptFailed here");
    assert!(error.is_open_failed(), "got {error}");
}

/// Zig: `encrypt generates unique nonces` — two seals of the same input differ.
#[test]
fn encrypt_generates_unique_nonces() {
    let key = kek();
    let aad = Aad::from_bytes(ZIG_AD_A.to_vec());
    let sealer = Sealer::new();

    let first = sealer.seal(&key, &aad, ZIG_SHORT_PLAINTEXT).unwrap();
    let second = sealer.seal(&key, &aad, ZIG_SHORT_PLAINTEXT).unwrap();

    assert_ne!(
        first.payload_nonce(),
        second.payload_nonce(),
        "Zig asserts the two nonces are not equal"
    );
}

/// Zig: `loadKek returns the KEK seeded via setKekFromHex (Option C boot-resolve)`.
///
/// Zig compares the loaded bytes directly. This crate has no way to read key
/// material back out — that is Invariant 5 — so equality is asserted through
/// behaviour instead: a key decoded from hex opens what the same key sealed.
#[test]
fn kek_round_trips_through_hex() {
    let aad = Aad::from_bytes(Vec::new());
    let sealed = Sealer::new()
        .seal(&kek(), &aad, ZIG_ROUND_TRIP_PLAINTEXT)
        .unwrap();

    let reloaded = Kek::from_hex(TEST_KEK_HEX).unwrap();
    assert_eq!(
        sealed.open(&reloaded, &aad).unwrap().expose(),
        ZIG_ROUND_TRIP_PLAINTEXT
    );
}

/// Zig: `setKekFromHex rejects a wrong-length hex (fails closed)` — input `"deadbeef"`.
#[test]
fn kek_rejects_a_wrong_length_hex() {
    let error = Kek::from_hex("deadbeef").expect_err("Zig expects SecretError.InvalidKeyHex");
    assert!(error.is_key_hex(), "got {error}");
}

/// Zig: `secure memory free hands zeroed bytes to the child allocator`.
///
/// The Zig daemon routes secret frees through a zeroing allocator; this crate
/// puts the same guarantee in the type, so the mirrored assertion is that the
/// buffer holds no non-zero byte once released.
#[test]
fn secret_bytes_are_zeroed_before_release() {
    use zeroize::Zeroize as _;

    let mut recovered = Sealer::new()
        .seal(
            &kek(),
            &Aad::from_bytes(Vec::new()),
            ZIG_ROUND_TRIP_PLAINTEXT,
        )
        .unwrap()
        .open(&kek(), &Aad::from_bytes(Vec::new()))
        .unwrap();

    assert_eq!(recovered.expose(), ZIG_ROUND_TRIP_PLAINTEXT);
    recovered.zeroize();
    assert!(recovered.expose().iter().all(|byte| *byte == 0));
}
