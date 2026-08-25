//! Dimension 1.3 — every tampered envelope fails closed, and nothing panics.
//!
//! The daemon links these crates and must not abort on a malformed row, so the
//! workspace denies `panic`, `unwrap` and `indexing_slicing` in library code.
//! These tests are the behavioural half of that: a flipped byte anywhere in the
//! envelope, a wrong key, a wrong workspace, a wrong name — each produces a
//! typed error, and the process survives.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use afd_crypto::secret::Kek;

const KEK_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const OTHER_KEK_HEX: &str = "ff02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const PLAINTEXT: &[u8] = b"{\"api_key\":\"not-a-real-key\"}";

fn kek() -> Kek {
    Kek::from_hex(KEK_HEX).unwrap()
}

fn sealed() -> (Envelope, Aad) {
    let aad = Aad::new("ws_0123", "openai");
    let envelope = Sealer::new().seal(&kek(), &aad, PLAINTEXT).unwrap();
    (envelope, aad)
}

/// The baseline the negative cases are measured against.
#[test]
fn test_envelope_round_trips() {
    let (envelope, aad) = sealed();
    let opened = envelope.open(&kek(), &aad).unwrap();
    assert_eq!(opened.expose(), PLAINTEXT);
    assert_eq!(envelope.kek_version(), 2);
}

/// Every component, flipped one bit at a time. Exhaustive over the six fields
/// rather than sampling one, because a reader that authenticates five of six is
/// exactly the bug this catches.
#[test]
fn test_envelope_rejects_tampered() {
    let (original, aad) = sealed();

    let mutations: Vec<(&str, Envelope)> = vec![
        (
            "wrapped dek",
            rebuild(&original, |p| flip(&mut p.wrapped_dek)),
        ),
        ("dek nonce", rebuild(&original, |p| flip(&mut p.dek_nonce))),
        ("dek tag", rebuild(&original, |p| flip(&mut p.dek_tag))),
        (
            "payload nonce",
            rebuild(&original, |p| flip(&mut p.payload_nonce)),
        ),
        (
            "payload ciphertext",
            rebuild(&original, |p| flip(&mut p.payload_ciphertext)),
        ),
        (
            "payload tag",
            rebuild(&original, |p| flip(&mut p.payload_tag)),
        ),
    ];

    for (component, tampered) in mutations {
        let error = tampered
            .open(&kek(), &aad)
            .expect_err("a flipped byte in the {component} must not open");
        assert!(
            error.is_open_failed(),
            "{component}: expected an open failure, got {error}"
        );
    }
}

/// A wrong key fails the same way a tampered tag does, and says no more.
#[test]
fn test_envelope_rejects_a_wrong_key() {
    let (envelope, aad) = sealed();
    let error = envelope
        .open(&Kek::from_hex(OTHER_KEK_HEX).unwrap(), &aad)
        .expect_err("a foreign key must not open the envelope");
    assert!(error.is_open_failed());
}

/// The associated data is load-bearing: a row lifted to another workspace or
/// renamed does not open, which is the property §Aad exists to provide.
#[test]
fn test_envelope_rejects_foreign_associated_data() {
    let (envelope, _) = sealed();

    for wrong in [
        Aad::new("ws_9999", "openai"),
        Aad::new("ws_0123", "anthropic"),
        Aad::versioned("ws_0123", "openai", 3),
    ] {
        let error = envelope
            .open(&kek(), &wrong)
            .expect_err("mismatched associated data must not open");
        assert!(error.is_open_failed(), "got {error}");
    }
}

/// A short or long fixed-width component is refused at construction, named.
#[test]
fn test_envelope_rejects_malformed_components() {
    let (good, _) = sealed();

    let short = Envelope::from_parts(
        good.wrapped_dek().to_vec(),
        &[0_u8; 11],
        good.dek_tag(),
        good.payload_nonce(),
        good.payload_ciphertext().to_vec(),
        good.payload_tag(),
        2,
    )
    .expect_err("an 11-byte nonce is not a nonce");
    assert!(short.is_malformed_envelope(), "got {short}");
    assert!(!short.is_open_failed());
}

/// A wrapped DEK of the wrong length is refused at construction, named.
///
/// The variable-width column is the payload ciphertext, not this one: the
/// wrapped DEK is a detached ciphertext over a 32-byte key and is 32 bytes or
/// the row is damaged. Truncating it must fail here, not deep inside the AEAD.
#[test]
fn test_envelope_rejects_a_wrong_length_wrapped_dek() {
    let (good, _) = sealed();

    let mut truncated = good.wrapped_dek().to_vec();
    truncated.truncate(31);

    for wrapped in [
        Vec::new(),
        truncated,
        [good.wrapped_dek(), &[0_u8]].concat(),
    ] {
        let actual = wrapped.len();
        let error = Envelope::from_parts(
            wrapped,
            good.dek_nonce(),
            good.dek_tag(),
            good.payload_nonce(),
            good.payload_ciphertext().to_vec(),
            good.payload_tag(),
            2,
        )
        .expect_err("only a 32-byte wrapped DEK is a wrapped key");
        assert!(error.is_malformed_envelope(), "{actual} bytes gave {error}");
        assert!(!error.is_open_failed(), "{actual} bytes gave {error}");
        assert_eq!(error.code().as_str(), "UZ-VAULT-001");
        assert!(
            error.to_string().contains("wrapped dek"),
            "the failing component must be named: {error}"
        );
    }
}

/// An unsupported version is refused before any byte is authenticated.
#[test]
fn test_envelope_rejects_unsupported_version() {
    let (good, _) = sealed();
    let error = Envelope::from_parts(
        good.wrapped_dek().to_vec(),
        good.dek_nonce(),
        good.dek_tag(),
        good.payload_nonce(),
        good.payload_ciphertext().to_vec(),
        good.payload_tag(),
        1,
    )
    .expect_err("version 1 is not supported");
    assert!(error.is_malformed_envelope());
    assert_eq!(error.code().as_str(), "UZ-VAULT-001");
}

/// An empty payload still produces a full, authenticating envelope.
#[test]
fn test_envelope_seals_an_empty_payload() {
    let aad = Aad::new("ws_0123", "empty");
    let envelope = Sealer::new().seal(&kek(), &aad, b"").unwrap();
    assert!(envelope.payload_ciphertext().is_empty());
    assert!(envelope.open(&kek(), &aad).unwrap().is_empty());
}

/// Two seals of the same plaintext differ, so nonce reuse would be visible.
#[test]
fn test_envelope_never_reuses_a_nonce() {
    let aad = Aad::new("ws_0123", "openai");
    let sealer = Sealer::new();
    let first = sealer.seal(&kek(), &aad, PLAINTEXT).unwrap();
    let second = sealer.seal(&kek(), &aad, PLAINTEXT).unwrap();

    assert_ne!(first.payload_nonce(), second.payload_nonce());
    assert_ne!(first.dek_nonce(), second.dek_nonce());
    assert_ne!(first.payload_ciphertext(), second.payload_ciphertext());
}

/// An entropy failure refuses to seal rather than falling back to weak bytes.
#[test]
fn test_envelope_refuses_to_seal_without_entropy() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.fail_next();
    let error = sealer
        .seal(&kek(), &Aad::new("ws_0123", "openai"), PLAINTEXT)
        .expect_err("a dead entropy source must refuse, not improvise");
    assert!(error.is_entropy(), "got {error}");
}

/// Mutable parts of an envelope, so a test can flip one and rebuild.
struct Parts {
    wrapped_dek: Vec<u8>,
    dek_nonce: [u8; 12],
    dek_tag: [u8; 16],
    payload_nonce: [u8; 12],
    payload_ciphertext: Vec<u8>,
    payload_tag: [u8; 16],
}

fn rebuild(source: &Envelope, mutate: impl FnOnce(&mut Parts)) -> Envelope {
    let mut parts = Parts {
        wrapped_dek: source.wrapped_dek().to_vec(),
        dek_nonce: *source.dek_nonce(),
        dek_tag: *source.dek_tag(),
        payload_nonce: *source.payload_nonce(),
        payload_ciphertext: source.payload_ciphertext().to_vec(),
        payload_tag: *source.payload_tag(),
    };
    mutate(&mut parts);
    Envelope::from_parts(
        parts.wrapped_dek,
        &parts.dek_nonce,
        &parts.dek_tag,
        &parts.payload_nonce,
        parts.payload_ciphertext,
        &parts.payload_tag,
        2,
    )
    .unwrap()
}

/// Flips the low bit of the first byte, or appends one if there is no byte.
fn flip(bytes: &mut [u8]) {
    if let Some(first) = bytes.first_mut() {
        *first ^= 0x01;
    }
}
