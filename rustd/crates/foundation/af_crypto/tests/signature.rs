//! A release signature verifies against its key and nothing else.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_crypto::signature::testing::EcdsaP256Signer;
use af_crypto::signature::verify_ecdsa_p256_sha256;

const MANIFEST: &[u8] = br#"{"arch":"arm64"}"#;

#[test]
fn test_a_signature_verifies_against_its_own_key() {
    let signer = EcdsaP256Signer::generate();
    let signature = signer.sign(MANIFEST);
    verify_ecdsa_p256_sha256(&signer.public_key_der(), MANIFEST, &signature).unwrap();
}

#[test]
fn test_another_message_or_key_or_a_flipped_bit_is_refused() {
    let signer = EcdsaP256Signer::generate();
    let key = signer.public_key_der();
    let signature = signer.sign(MANIFEST);
    let mut flipped = signature.clone();
    if let Some(last) = flipped.last_mut() {
        *last ^= 1;
    }
    let other = EcdsaP256Signer::generate().public_key_der();
    for (label, key, message, signature) in [
        (
            "another message",
            &key,
            &br#"{"arch":"amd64"}"#[..],
            &signature,
        ),
        ("another key", &other, MANIFEST, &signature),
        ("a flipped bit", &key, MANIFEST, &flipped),
    ] {
        let refused = verify_ecdsa_p256_sha256(key, message, signature).expect_err(label);
        assert!(refused.is_signature_mismatch(), "{label}");
    }
}
