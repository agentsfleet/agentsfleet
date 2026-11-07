//! Dimension 1.4 — key material redacts in every rendering and zeroes on drop.
//!
//! `M-PUBLIC-DEBUG` asks for exactly this file: sensitive types still implement
//! `Debug`, but through a hand-written implementation whose redaction has a
//! test behind it, so a later `#[derive(Debug)]` added for convenience fails
//! here instead of leaking a key into a log line.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::KEY_LEN;
use afd_crypto::secret::{Dek, Kek, SecretBytes};

/// A key whose hex is distinctive enough to find in any rendering.
const KEK_HEX: &str = "deadbeef00112233445566778899aabbccddeeff0123456789abcdef01234567";

#[test]
fn test_secret_types_redact() {
    let kek = Kek::from_hex(KEK_HEX).unwrap();
    let dek = Dek::from_bytes([0xAB; KEY_LEN]);
    let plaintext = SecretBytes::new(b"sk-super-secret-value".to_vec());

    for (label, debug, display) in [
        ("Kek", format!("{kek:?}"), format!("{kek}")),
        ("Dek", format!("{dek:?}"), format!("{dek}")),
        (
            "SecretBytes",
            format!("{plaintext:?}"),
            format!("{plaintext}"),
        ),
    ] {
        for rendered in [&debug, &display] {
            assert!(
                rendered.contains(label),
                "{label} rendering should name its type, got {rendered}"
            );
            assert!(
                rendered.contains("redacted"),
                "{label} rendering should say it is redacted, got {rendered}"
            );
        }
    }

    // The material itself, in every form it could plausibly leak as.
    let renderings = format!("{kek:?}{kek}{dek:?}{dek}{plaintext:?}{plaintext}");
    assert!(!renderings.contains(KEK_HEX), "the key hex leaked");
    assert!(!renderings.contains("deadbeef"), "a key prefix leaked");
    assert!(
        !renderings.contains("sk-super-secret-value"),
        "plaintext leaked"
    );
    assert!(
        !renderings.contains("171"),
        "a byte value leaked as decimal"
    );
    assert!(!renderings.contains("ab, ab"), "bytes leaked as a slice");
}

/// The hex decoder is the boundary `ENCRYPTION_MASTER_KEY` crosses, so every
/// way an operator can get it wrong is a named error rather than a panic.
#[test]
fn test_kek_from_hex_rejects_malformed_keys() {
    for bad in [
        "",
        "deadbeef",
        &KEK_HEX[..KEK_HEX.len() - 1],
        &format!("{KEK_HEX}0"),
        &"g".repeat(KEY_LEN * 2),
        &format!("{}zz", &KEK_HEX[..KEK_HEX.len() - 2]),
    ] {
        let error = Kek::from_hex(bad).expect_err("malformed key must be refused");
        assert!(error.is_key_hex(), "{bad:?} produced {error}");
        assert_eq!(error.code().as_str(), "UZ-INTERNAL-003");
    }
}

/// Upper-case hex decodes to the same key, because operators paste both.
#[test]
fn test_kek_from_hex_accepts_either_case() {
    let lower = Kek::from_hex(KEK_HEX).unwrap();
    let upper = Kek::from_hex(&KEK_HEX.to_ascii_uppercase()).unwrap();
    // Compared through behaviour rather than by exposing bytes: the same key
    // seals and opens interchangeably.
    let aad = afd_crypto::aad::Aad::new("ws_1", "k");
    let envelope = afd_crypto::envelope::Sealer::new()
        .seal(&lower, &aad, b"payload")
        .unwrap();
    assert_eq!(envelope.open(&upper, &aad).unwrap().expose(), b"payload");

    // And the rejection an operator reads must not blame a case this decoder
    // accepts — a message that says "lowercase" sends them editing a key that
    // was only ever the wrong length.
    let mut short = KEK_HEX.to_ascii_uppercase();
    short.pop();
    let error = Kek::from_hex(&short).expect_err("63 characters is not a key");
    assert!(
        !error.to_string().contains("lowercase"),
        "the length error must not blame capitalisation: {error}"
    );
}

/// A wrong-length unwrapped key is refused with the component named.
#[test]
fn test_dek_from_slice_rejects_a_wrong_length() {
    let error = Dek::from_slice(&[0_u8; 31]).expect_err("31 bytes is not a key");
    assert!(error.is_malformed_envelope(), "got {error}");
    Dek::from_slice(&[0_u8; KEY_LEN]).expect("a full-width key is accepted");
}

/// Recovered plaintext reports its own shape without exposing it.
#[test]
fn test_secret_bytes_reports_length_without_exposing() {
    let empty = SecretBytes::new(Vec::new());
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);

    let filled = SecretBytes::new(b"abc".to_vec());
    assert!(!filled.is_empty());
    assert_eq!(filled.len(), 3);
    assert_eq!(filled.expose(), b"abc");
}

/// Zeroing on demand clears the buffer, which is what drop relies on.
///
/// Observing memory after a drop is undefined behaviour, so the drop path is
/// proven by exercising the same `Zeroize` implementation the `Drop` impl calls
/// rather than by reading freed memory.
#[test]
fn test_secret_bytes_zeroize_clears_the_buffer() {
    use zeroize::Zeroize as _;
    let mut secret = SecretBytes::new(b"sk-super-secret-value".to_vec());
    secret.zeroize();
    assert!(
        secret.expose().iter().all(|byte| *byte == 0),
        "zeroize must leave no non-zero byte"
    );
}
