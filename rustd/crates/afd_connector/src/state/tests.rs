//! What a state must and must not verify as.
//!
//! Negative-heavy on purpose: the happy path is one assertion and every other
//! case here is a way somebody could get a connect completed that they did not
//! start. `state.zig`'s own suite proves the same set, and this mirrors it
//! case for case so the two daemons cannot drift on what they accept.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::secret::SecretBytes;

use super::{Rejected, sign, verify};
use crate::provider::Provider;
use crate::registry::STATE_TTL_SECONDS;

/// A readable, low-entropy signing key — any string keys an HMAC, and a
/// high-entropy hex value trips the secret scanner's generic-api-key rule.
const SECRET: &str = "connector-state-signing-secret-fixture";

/// The workspace every case in this file connects.
const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";

/// The person who starts the connect.
const STARTER: &str = "user_test";

/// Somebody else, authenticated, who did not start it.
const BYSTANDER: &str = "user_other";

/// The nonce fixture — a state's single-use half, supplied rather than drawn.
const NONCE: &str = "deadbeefdeadbeefdeadbeefdeadbeef";

/// The instant every case signs at.
const NOW: i64 = 1_700_000_000_000;

/// The signing key, as the secret type the production path holds.
fn secret() -> SecretBytes {
    SecretBytes::new(SECRET.as_bytes().to_vec())
}

/// A state minted for the starter at [`NOW`].
fn minted(provider: Provider) -> String {
    sign(
        provider.state_binding(),
        &secret(),
        WORKSPACE,
        STARTER,
        NONCE,
        UnixMillis::from_millis(NOW),
    )
}

/// A round-trip carries back the workspace, the nonce, and who started it.
#[test]
fn a_signed_state_round_trips_its_workspace_nonce_and_starter() {
    let provider = Provider::Slack;
    let state = minted(provider);

    let verified = verify(
        provider.state_binding(),
        &secret(),
        &state,
        UnixMillis::from_millis(NOW),
    )
    .expect("a freshly minted state verifies");

    assert_eq!(verified.workspace(), WORKSPACE);
    assert_eq!(verified.nonce(), NONCE);
    assert!(verified.subject_matches(provider.state_binding(), &secret(), STARTER));
}

/// A different authenticated person is not the starter.
///
/// The check that stops an authenticated bystander from completing — and
/// consuming — somebody else's in-flight connect by replaying its callback URL.
#[test]
fn a_different_authenticated_person_is_not_the_starter() {
    let provider = Provider::Slack;
    let state = minted(provider);

    let verified = verify(
        provider.state_binding(),
        &secret(),
        &state,
        UnixMillis::from_millis(NOW),
    )
    .expect("a freshly minted state verifies");

    assert!(!verified.subject_matches(provider.state_binding(), &secret(), BYSTANDER));
}

/// A flipped tag character does not verify.
#[test]
fn a_tampered_authentication_tag_is_refused() {
    let mut state = minted(Provider::Slack);
    let flipped = if state.ends_with('a') { 'b' } else { 'a' };
    state.pop();
    state.push(flipped);

    let refused = verify(
        Provider::Slack.state_binding(),
        &secret(),
        &state,
        UnixMillis::from_millis(NOW),
    );

    assert_eq!(refused.err(), Some(Rejected::BadSignature));
}

/// A state signed under another deployment's secret does not verify.
#[test]
fn a_state_signed_under_a_foreign_secret_is_refused() {
    let state = minted(Provider::Slack);
    let other = SecretBytes::new(b"a-different-deployments-secret".to_vec());

    let refused = verify(
        Provider::Slack.state_binding(),
        &other,
        &state,
        UnixMillis::from_millis(NOW),
    );

    assert_eq!(refused.err(), Some(Rejected::BadSignature));
}

/// One connector's state does not verify as another's.
///
/// The domain separation `crate::registry`'s suite proves the prefixes for,
/// asserted end to end: same secret, same payload, different provider.
#[test]
fn a_state_minted_for_one_connector_does_not_verify_as_another() {
    let state = minted(Provider::Slack);

    let refused = verify(
        Provider::Jira.state_binding(),
        &secret(),
        &state,
        UnixMillis::from_millis(NOW),
    );

    assert_eq!(refused.err(), Some(Rejected::BadSignature));
}

/// The window's two edges, either side of the documented lifetime.
///
/// The last millisecond inside verifies and the first one outside does not —
/// pinned together so a change to the boundary cannot pass by moving one of
/// them.
#[test]
fn the_expiry_window_holds_at_both_of_its_edges() {
    let state = minted(Provider::Slack);
    // pin test: literal is the contract
    let window = i64::from(STATE_TTL_SECONDS) * 1_000;
    let binding = Provider::Slack.state_binding();

    let inside = verify(
        binding,
        &secret(),
        &state,
        UnixMillis::from_millis(NOW + window),
    );
    let outside = verify(
        binding,
        &secret(),
        &state,
        UnixMillis::from_millis(NOW + window + 1),
    );

    assert!(
        inside.is_ok(),
        "the last instant inside the window verifies"
    );
    assert_eq!(outside.err(), Some(Rejected::Expired));
}

/// Nothing that is not a state verifies as one.
///
/// Refusal is the property; WHICH refusal is not, and deliberately so. A `.`
/// splits into an empty payload and an empty tag, which is state-SHAPED and so
/// dies at the signature rather than at the parse — and a caller must not be
/// able to tell those apart anyway, because both answer one code. What this
/// pins is that none of them gets through.
#[test]
fn nothing_that_is_not_a_state_verifies_as_one() {
    let binding = Provider::Slack.state_binding();

    for presented in ["", "not-a-state", ".", "....", "!!!.deadbeef", "e30.zz"] {
        let refused = verify(binding, &secret(), presented, UnixMillis::from_millis(NOW));

        assert!(refused.is_err(), "`{presented}` must not verify");
    }
}

/// A state carrying a FIFTH field is refused even though it is signed.
///
/// That is a newer daemon's payload. Reading the four fields this build knows
/// and ignoring whatever the fifth was for is how a format change becomes a
/// silent behaviour change, so the parse refuses rather than truncates.
#[test]
fn a_correctly_signed_payload_with_an_extra_field_is_malformed() {
    let binding = Provider::Slack.state_binding();
    let payload = format!("{WORKSPACE}|tag|{NONCE}|{}|extra", NOW + 60_000);
    let mac = super::mac_hex(binding, &secret(), payload.as_bytes());
    let encoded =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &payload);

    let refused = verify(
        binding,
        &secret(),
        &format!("{encoded}.{mac}"),
        UnixMillis::from_millis(NOW),
    );

    assert_eq!(refused.err(), Some(Rejected::Malformed));
}

/// Each rejection names itself for the log, and no two share a word.
///
/// The reasons are what an operator reads to tell a forgery from clock skew,
/// so two collapsing onto one word would make the log strictly less useful
/// than the single code the caller already answers.
#[test]
fn every_rejection_carries_its_own_word_for_the_log() {
    let reasons = [
        Rejected::Malformed.reason(),
        Rejected::BadSignature.reason(),
        Rejected::Expired.reason(),
        Rejected::ForeignSubject.reason(),
    ];

    for (index, one) in reasons.iter().enumerate() {
        assert!(!one.is_empty());
        let later = reasons.get(index + 1..).unwrap_or_default();
        assert!(!later.contains(one), "`{one}` is reused");
    }
}
