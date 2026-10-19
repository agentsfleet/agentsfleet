//! The seal path drawing through a scripted entropy source: the order it draws
//! in, and that a draw refused part way through aborts the envelope.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;

const KEK_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

fn kek() -> Kek {
    Kek::from_hex(KEK_HEX).unwrap()
}

/// Queued bytes reach the envelope in the order the seal path draws them:
/// the Data Encryption Key, then its nonce, then the payload nonce.
#[test]
fn test_mock_entropy_feeds_in_draw_order() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.push_bytes(&[0xAA; 32]);
    entropy.push_bytes(&[0xBB; 12]);
    entropy.push_bytes(&[0xCC; 12]);

    let envelope = sealer
        .seal(&kek(), &Aad::new("ws_1", "k"), b"payload")
        .unwrap();

    assert_eq!(envelope.dek_nonce(), &[0xBB; 12]);
    assert_eq!(envelope.payload_nonce(), &[0xCC; 12]);
}

/// A draw that fails PART WAY through a seal aborts it, rather than continuing
/// with whatever bytes were already in hand.
///
/// The seal path draws three times. The first two here succeed and the third —
/// the payload nonce — is refused, which is the only way to reach the error
/// propagation on the second encryption. A version that swallowed it would
/// produce an envelope whose payload nonce was silently left zeroed.
#[test]
fn test_mock_entropy_failure_mid_seal_aborts_the_envelope() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.push_bytes(&[0xAA; 32]); // the Data Encryption Key
    entropy.push_bytes(&[0xBB; 12]); // the nonce wrapping it
    entropy.push_bytes(&[0xCC; 7]); // the payload nonce — wrong width on purpose

    let error = sealer
        .seal(&kek(), &Aad::new("ws_1", "k"), b"payload")
        .expect_err("a refused third draw must abort the seal");
    assert!(error.is_malformed_envelope(), "got {error}");
}

/// A draw with nothing queued fills with the fixed counting pattern, wrapping
/// at 251, rather than silently going random: two unscripted mocks agree byte
/// for byte, so a test that forgot to queue still reproduces.
#[test]
fn test_mock_entropy_unscripted_draw_is_the_counting_pattern() {
    let (first, _ctrl) = afd_crypto::entropy::Entropy::new_mocked();
    let (second, _other) = afd_crypto::entropy::Entropy::new_mocked();
    let mut drawn = [0xFF_u8; 300];
    let mut again = [0x00_u8; 300];

    first.fill(&mut drawn).unwrap();
    second.fill(&mut again).unwrap();

    assert_eq!(drawn, again);
    assert_eq!(drawn[0], 0);
    assert_eq!(drawn[250], 250);
    assert_eq!(drawn[251], 0, "the pattern wraps at 251");
    assert_eq!(drawn[299], 48);
}
