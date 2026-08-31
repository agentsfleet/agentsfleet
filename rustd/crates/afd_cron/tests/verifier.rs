//! The wall a scheduled fire crosses, and every way across it that is refused.
//!
//! Dimension 3.2's signature half. The fire route is reachable by anyone who
//! finds the URL — it carries no bearer, because the scheduler has none to
//! present — so the token IS the authentication and this is the file that says
//! what the token has to prove.
//!
//! # Every token here is minted, not pasted
//!
//! A fixture string can only assert one shape, and the interesting cases are
//! all "a token that differs from a good one in exactly ONE claim": the right
//! signature over the wrong body, the right body under the wrong key, the right
//! everything for a different daemon. Minting is what lets each case vary one
//! thing and hold the rest, so a refusal names the claim that caused it rather
//! than whichever check happened to run first.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_cron::verifier::{MAX_TOKEN_BYTES, verify_at};
use afd_cron::{SigningKeys, Unverified};
use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use sha2::{Digest as _, Sha256};

/// The destination this fixture deployment's schedules are registered against.
const DESTINATION: &str = "https://api.fixture.test/v1/ingress/qstash/schedules";

/// The key the scheduler is signing with in these tests.
const CURRENT_KEY: &str = "fixture-current-signing-key";

/// The key it rotates to next.
const NEXT_KEY: &str = "fixture-next-signing-key";

/// The delivery body a fire carries.
const BODY: &[u8] = br#"{"schedule_id":"01J0000000000000000000000A"}"#;

/// Seconds since the epoch, for the `exp`/`nbf` the crate reads a system clock
/// against. There is no seam to hand an instant in — see `verify_at`'s note —
/// so the fixtures are placed generously around now rather than pinned.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the fixture clock is after the epoch")
        .as_secs()
}

/// The claims a fire token carries, as the scheduler mints them.
#[derive(Serialize)]
struct FireClaims {
    iss: String,
    sub: String,
    exp: u64,
    nbf: u64,
    jti: String,
    body: String,
}

impl FireClaims {
    /// A token the wall should believe.
    fn good() -> Self {
        Self {
            iss: "Upstash".to_owned(),
            sub: DESTINATION.to_owned(),
            exp: now() + 300,
            nbf: now() - 10,
            jti: "msg_fixture_0001".to_owned(),
            body: digest_of(BODY),
        }
    }
}

/// The `body` claim the scheduler puts in a token: base64url, unpadded.
fn digest_of(body: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(body))
}

/// Signs `claims` with `key`, as the scheduler would.
fn mint(claims: &FireClaims, key: &str) -> String {
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(key.as_bytes()),
    )
    .expect("the fixture claims serialise")
}

/// Both of this deployment's keys.
fn keys() -> SigningKeys {
    SigningKeys {
        current: CURRENT_KEY.to_owned(),
        next: NEXT_KEY.to_owned(),
    }
}

#[test]
fn a_token_the_scheduler_minted_is_believed_and_carries_its_delivery_id() {
    let token = mint(&FireClaims::good(), CURRENT_KEY);

    let fire = verify_at(&keys(), DESTINATION, &token, BODY)
        .expect("a token signed by the current key over this body is genuine");

    assert_eq!(
        fire.message_id, "msg_fixture_0001",
        "the delivery id travels out of the token, because it is the claim key \
         a replay is suppressed under"
    );
}

/// Both keys are tried, which is what keeps a rotation from being an outage:
/// the scheduler moves to the next key before this daemon is told about it.
#[test]
fn a_token_signed_with_the_next_key_is_believed_too() {
    let token = mint(&FireClaims::good(), NEXT_KEY);

    verify_at(&keys(), DESTINATION, &token, BODY)
        .expect("the next key is the scheduler's too, so a rotation is not an outage");
}

/// Fail-closed. With no key there is nothing to check against, and accepting
/// the fire would let anyone who found the URL wake every fleet on it.
#[test]
fn a_deployment_holding_no_key_refuses_before_it_reads_the_token() {
    let token = mint(&FireClaims::good(), CURRENT_KEY);
    let none = SigningKeys {
        current: String::new(),
        next: String::new(),
    };

    assert_eq!(
        verify_at(&none, DESTINATION, &token, BODY).expect_err("this token must not be believed"),
        Unverified::KeysMissing,
        "a genuine token is still refused by a deployment that cannot check it"
    );
}

/// A deployment mid-rotation holds one key, and one key still verifies.
#[test]
fn one_configured_key_is_enough_to_verify() {
    let only_current = SigningKeys {
        current: CURRENT_KEY.to_owned(),
        next: String::new(),
    };

    verify_at(
        &only_current,
        DESTINATION,
        &mint(&FireClaims::good(), CURRENT_KEY),
        BODY,
    )
    .expect("an empty second key is skipped, not treated as a key that matches nothing");
}

#[test]
fn an_empty_or_oversized_token_is_malformed() {
    assert_eq!(
        verify_at(&keys(), DESTINATION, "", BODY).expect_err("this token must not be believed"),
        Unverified::Malformed
    );

    let oversized = "a".repeat(MAX_TOKEN_BYTES + 1);
    assert_eq!(
        verify_at(&keys(), DESTINATION, &oversized, BODY)
            .expect_err("this token must not be believed"),
        Unverified::Malformed,
        "the bound is on the work one unauthenticated request can ask of the \
         decoder, so it is checked before decoding"
    );
}

#[test]
fn a_token_signed_with_a_key_this_deployment_does_not_hold_is_refused() {
    let forged = mint(&FireClaims::good(), "not-a-key-this-deployment-holds");

    assert_eq!(
        verify_at(&keys(), DESTINATION, &forged, BODY)
            .expect_err("this token must not be believed"),
        Unverified::SignatureInvalid
    );
}

/// The subject is the destination the schedule was registered against, so a
/// token minted for another deployment fails here rather than waking a fleet
/// on this one.
#[test]
fn a_token_minted_for_another_daemon_is_refused() {
    let mut claims = FireClaims::good();
    claims.sub = "https://someone-else.test/v1/ingress/qstash/schedules".to_owned();
    let token = mint(&claims, CURRENT_KEY);

    assert_eq!(
        verify_at(&keys(), DESTINATION, &token, BODY).expect_err("this token must not be believed"),
        Unverified::SignatureInvalid,
        "a genuine token for the wrong destination must not fire this deployment"
    );
}

#[test]
fn a_token_from_an_issuer_that_is_not_the_scheduler_is_refused() {
    let mut claims = FireClaims::good();
    claims.iss = "SomebodyElse".to_owned();

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, CURRENT_KEY), BODY)
            .expect_err("this token must not be believed"),
        Unverified::SignatureInvalid
    );
}

#[test]
fn an_expired_token_is_refused_with_no_leeway() {
    let mut claims = FireClaims::good();
    claims.exp = now() - 1;
    claims.nbf = now() - 600;

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, CURRENT_KEY), BODY)
            .expect_err("this token must not be believed"),
        Unverified::SignatureInvalid,
        "leeway is zero on purpose: a tolerance is a window a replayed token \
         lives inside"
    );
}

#[test]
fn a_token_whose_window_has_not_opened_is_refused() {
    let mut claims = FireClaims::good();
    claims.nbf = now() + 600;
    claims.exp = now() + 1200;

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, CURRENT_KEY), BODY)
            .expect_err("this token must not be believed"),
        Unverified::SignatureInvalid
    );
}

/// The one refusal that means somebody is replaying a captured token: the
/// signature is genuine and the bytes it was minted over are not the bytes that
/// arrived.
#[test]
fn a_genuine_token_over_different_bytes_is_a_body_mismatch() {
    let token = mint(&FireClaims::good(), CURRENT_KEY);

    assert_eq!(
        verify_at(
            &keys(),
            DESTINATION,
            &token,
            b"{\"schedule_id\":\"substituted\"}"
        )
        .expect_err("this token must not be believed"),
        Unverified::BodyMismatch,
        "the token is real, so this is not a signature failure — it is the \
         body being swapped underneath a captured one"
    );
}

/// A body that differs by one byte is still a different body; the compare is
/// over a digest and short-circuits on length rather than content.
#[test]
fn a_body_differing_by_one_byte_is_still_a_mismatch() {
    let token = mint(&FireClaims::good(), CURRENT_KEY);
    let mut altered = BODY.to_vec();
    let last = altered.last_mut().expect("the fixture body is not empty");
    *last = b'B';

    assert_eq!(
        verify_at(&keys(), DESTINATION, &token, &altered)
            .expect_err("this token must not be believed"),
        Unverified::BodyMismatch
    );
}

/// The reasons are what an operator greps for when a schedule stops firing, so
/// each is a stable word and no two share one.
#[test]
fn every_refusal_reads_under_its_own_stable_reason() {
    let reasons = [
        (Unverified::KeysMissing, "signing_keys_absent"),
        (Unverified::Malformed, "token_malformed"),
        (Unverified::SignatureInvalid, "signature_invalid"),
        (Unverified::WrongTarget, "wrong_target"),
        (Unverified::OutsideWindow, "outside_window"),
        (Unverified::BodyMismatch, "body_mismatch"),
    ];

    for (variant, expected) in reasons {
        assert_eq!(variant.reason(), expected);
    }

    for (index, (_variant, reason)) in reasons.iter().enumerate() {
        let duplicate = reasons
            .iter()
            .skip(index + 1)
            .any(|(_other, other_reason)| other_reason == reason);
        assert!(!duplicate, "`{reason}` is the reason for two variants");
    }
}
