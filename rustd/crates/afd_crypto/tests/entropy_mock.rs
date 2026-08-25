//! The mock entropy source is production code for tests, so it is tested too.
//!
//! A mock that silently misbehaves turns every test built on it into a lie —
//! `M-TAUTOLOGICAL-TESTS`' concern from the other direction. These assert the
//! three behaviours the rest of the suite relies on: queued bytes come back
//! exactly, an exhausted queue is deterministic rather than random, and a
//! mis-sized push is refused rather than truncated.
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

/// An exhausted queue fills deterministically instead of falling back to the
/// operating system — a silent switch to real randomness would make a failing
/// test pass on the next run.
#[test]
fn test_mock_entropy_is_deterministic_when_the_queue_runs_dry() {
    let aad = Aad::new("ws_1", "k");

    let (first_sealer, _first_ctrl) = Sealer::new_mocked();
    let first = first_sealer.seal(&kek(), &aad, b"payload").unwrap();

    let (second_sealer, _second_ctrl) = Sealer::new_mocked();
    let second = second_sealer.seal(&kek(), &aad, b"payload").unwrap();

    assert_eq!(
        first.payload_nonce(),
        second.payload_nonce(),
        "an empty queue must produce the same bytes every run"
    );
    assert_eq!(first.payload_ciphertext(), second.payload_ciphertext());
}

/// A mis-sized push is refused, naming the mismatch, rather than being padded
/// or truncated into something that would quietly seal under the wrong nonce.
#[test]
fn test_mock_entropy_rejects_a_wrong_width_push() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.push_bytes(&[0xAA; 7]); // the seal path asks for 32 first

    let error = sealer
        .seal(&kek(), &Aad::new("ws_1", "k"), b"payload")
        .expect_err("a 7-byte push cannot satisfy a 32-byte draw");
    assert!(error.is_malformed_envelope(), "got {error}");
}

/// The failure switch fires once and then clears, so a later draw succeeds.
#[test]
fn test_mock_entropy_failure_is_one_shot() {
    let (sealer, entropy) = Sealer::new_mocked();
    entropy.fail_next();

    let aad = Aad::new("ws_1", "k");
    sealer
        .seal(&kek(), &aad, b"payload")
        .expect_err("the armed failure switch must refuse this draw");

    let recovered = sealer
        .seal(&kek(), &aad, b"payload")
        .expect("the failure switch must clear after one draw");
    assert_eq!(recovered.open(&kek(), &aad).unwrap().expose(), b"payload");
}

/// The controller is a shared handle, so pushes through a clone are seen.
///
/// `M-SERVICES-CLONE`: the sealer's copy and the caller's copy must be the
/// same state, or a test that queues bytes would silently drive nothing.
#[test]
fn test_mock_entropy_controller_is_a_shared_handle() {
    let (sealer, entropy) = Sealer::new_mocked();
    let clone = entropy.clone();
    clone.push_bytes(&[0x11; 32]);
    clone.push_bytes(&[0x22; 12]);
    clone.push_bytes(&[0x33; 12]);

    let envelope = sealer
        .seal(&kek(), &Aad::new("ws_1", "k"), b"payload")
        .unwrap();
    assert_eq!(envelope.payload_nonce(), &[0x33; 12]);
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
