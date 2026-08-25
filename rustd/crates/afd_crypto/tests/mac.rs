//! Message authentication: correct codes verify, wrong ones do not, in constant time.
//!
//! RULE CTM. The type deliberately does not implement `PartialEq`, so there is
//! no `==` to reach for; [`Mac256::verify`] is the only comparison, and it runs
//! in time independent of where two codes first differ.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::mac::{MAC_LEN, Mac256};
use afd_crypto::secret::Kek;

const KEK_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const OTHER_HEX: &str = "ff02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

fn kek(hex: &str) -> Kek {
    Kek::from_hex(hex).unwrap()
}

#[test]
fn test_mac_verifies_a_matching_code() {
    let key = kek(KEK_HEX);
    let first = Mac256::compute(&key, b"agt_t_example");
    let second = Mac256::compute(&key, b"agt_t_example");
    first
        .verify(&second)
        .expect("the same message under the same key verifies");
}

#[test]
fn test_mac_rejects_a_different_message() {
    let key = kek(KEK_HEX);
    let error = Mac256::compute(&key, b"agt_t_example")
        .verify(&Mac256::compute(&key, b"agt_t_other"))
        .expect_err("a different message must not verify");
    assert!(error.is_mac_mismatch(), "got {error}");
}

#[test]
fn test_mac_rejects_a_different_key() {
    let error = Mac256::compute(&kek(KEK_HEX), b"same message")
        .verify(&Mac256::compute(&kek(OTHER_HEX), b"same message"))
        .expect_err("a different key must not verify");
    assert!(error.is_mac_mismatch());
}

/// The code is deterministic, which is what makes it usable as a lookup key.
#[test]
fn test_mac_is_deterministic_and_full_width() {
    let key = kek(KEK_HEX);
    let mac = Mac256::compute(&key, b"agt_t_example");
    assert_eq!(mac.as_bytes().len(), MAC_LEN);
    assert_eq!(mac.to_hex().len(), MAC_LEN * 2);
    assert_eq!(
        mac.to_hex(),
        Mac256::compute(&key, b"agt_t_example").to_hex()
    );
}

/// Round-trips through storage: the hex form rebuilds a verifying code.
#[test]
fn test_mac_round_trips_through_stored_bytes() {
    let key = kek(KEK_HEX);
    let original = Mac256::compute(&key, b"agt_t_example");
    let restored = Mac256::from_slice(original.as_bytes()).unwrap();
    original
        .verify(&restored)
        .expect("a code rebuilt from its bytes verifies");
}

/// A truncated stored code is refused rather than compared short.
#[test]
fn test_mac_from_slice_rejects_a_wrong_length() {
    let error = Mac256::from_slice(&[0_u8; 31]).expect_err("31 bytes is not a code");
    assert!(error.is_malformed_envelope(), "got {error}");
    Mac256::from_slice(&[0_u8; MAC_LEN]).expect("a full-width code is accepted");
}

/// The rendering shows the digest, which is not the secret behind it.
#[test]
fn test_mac_debug_shows_the_digest() {
    let mac = Mac256::compute(&kek(KEK_HEX), b"agt_t_example");
    let rendered = format!("{mac:?}");
    assert!(rendered.starts_with("Mac256("), "got {rendered}");
    assert!(rendered.contains(&mac.to_hex()), "got {rendered}");
}
