//! The wall a scheduled fire crosses, and every way across it that is refused.
//!
//! The signature half of the fire wall. The fire route is reachable by anyone who
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

#[path = "support/fire_token.rs"]
mod fire_token;

use afd_cron::SigningKeys;
use afd_cron::verifier::{MAX_TOKEN_BYTES, verify_at};

use self::fire_token::{BODY, DESTINATION, FireClaims, mint, now};

/// The key the scheduler is signing with in these tests.
const CURRENT_KEY: &str = "fixture-current-signing-key";

/// The key it rotates to next.
const NEXT_KEY: &str = "fixture-next-signing-key";

/// Both of this deployment's keys.
fn keys() -> SigningKeys {
    SigningKeys {
        current: af_crypto::secret::SecretString::new(CURRENT_KEY.to_owned()),
        next: af_crypto::secret::SecretString::new(NEXT_KEY.to_owned()),
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
        current: af_crypto::secret::SecretString::new(String::new()),
        next: af_crypto::secret::SecretString::new(String::new()),
    };

    assert_eq!(
        verify_at(&none, DESTINATION, &token, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signing_keys_absent",
        "a genuine token is still refused by a deployment that cannot check it"
    );
}

/// A deployment mid-rotation holds one key, and one key still verifies.
#[test]
fn one_configured_key_is_enough_to_verify() {
    let only_current = SigningKeys {
        current: af_crypto::secret::SecretString::new(CURRENT_KEY.to_owned()),
        next: af_crypto::secret::SecretString::new(String::new()),
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
        verify_at(&keys(), DESTINATION, "", BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_malformed"
    );

    let oversized = "a".repeat(MAX_TOKEN_BYTES + 1);
    assert_eq!(
        verify_at(&keys(), DESTINATION, &oversized, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_malformed",
        "the bound is on the work one unauthenticated request can ask of the \
         decoder, so it is checked before decoding"
    );
}

/// The compact form a signed token takes is three base64url segments joined by
/// dots. Anything else is refused as malformed before a key is tried, so a
/// stranger's guess is never mistaken for a forged signature.
#[test]
fn a_string_that_is_not_three_dotted_segments_is_malformed() {
    for token in [
        "garbage",
        "two.segments",
        "four.dotted.segments.here",
        ".empty.first",
        "empty..middle",
        "empty.last.",
        "has space.in.it",
        "padded.base64.token==",
        "line\nbreak.in.token",
        "non-ascii.tök.en",
    ] {
        assert_eq!(
            verify_at(&keys(), DESTINATION, token, BODY)
                .expect_err("this token must not be believed")
                .refusal_word(),
            "token_malformed",
            "{token:?}"
        );
    }
}

/// The shape check is not a signature check: a token shaped right and signed
/// by nobody this deployment trusts is still a forgery, not a malformed token.
#[test]
fn a_token_shaped_right_but_signed_by_nobody_is_a_forgery_not_malformed() {
    assert_eq!(
        verify_at(&keys(), DESTINATION, "aaa.bbb.ccc", BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid"
    );
}

#[test]
fn a_token_signed_with_a_key_this_deployment_does_not_hold_is_refused() {
    let forged = mint(&FireClaims::good(), "not-a-key-this-deployment-holds");

    assert_eq!(
        verify_at(&keys(), DESTINATION, &forged, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid"
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
        verify_at(&keys(), DESTINATION, &token, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid",
        "a genuine token for the wrong destination must not fire this deployment"
    );
}

#[test]
fn a_token_from_an_issuer_that_is_not_the_scheduler_is_refused() {
    let mut claims = FireClaims::good();
    claims.iss = "SomebodyElse".to_owned();

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, CURRENT_KEY), BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid"
    );
}

#[test]
fn an_expired_token_is_refused_with_no_leeway() {
    let mut claims = FireClaims::good();
    claims.exp = now() - 1;
    claims.nbf = now() - 600;

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, CURRENT_KEY), BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid",
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
            .expect_err("this token must not be believed")
            .refusal_word(),
        "signature_invalid"
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
        .expect_err("this token must not be believed")
        .refusal_word(),
        "body_mismatch",
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
            .expect_err("this token must not be believed")
            .refusal_word(),
        "body_mismatch"
    );
}

/// The reasons are what an operator greps for when a schedule stops firing, so
/// each is a stable word and no two share one.
#[test]
fn every_refusal_reads_under_its_own_stable_reason() {
    let good = mint(&FireClaims::good(), CURRENT_KEY);
    let none = SigningKeys {
        current: af_crypto::secret::SecretString::new(String::new()),
        next: af_crypto::secret::SecretString::new(String::new()),
    };
    let refusals = [
        (
            verify_at(&none, DESTINATION, &good, BODY),
            "signing_keys_absent",
        ),
        (verify_at(&keys(), DESTINATION, "", BODY), "token_malformed"),
        (
            verify_at(&keys(), DESTINATION, &mint(&FireClaims::good(), "x"), BODY),
            "signature_invalid",
        ),
        (
            verify_at(&keys(), DESTINATION, &good, b"swapped"),
            "body_mismatch",
        ),
    ];

    let reasons: Vec<&str> = refusals
        .into_iter()
        .map(|(refused, expected)| {
            let reason = refused
                .expect_err("this token must not be believed")
                .refusal_word();
            assert_eq!(reason, expected);
            reason
        })
        .collect();

    for (index, reason) in reasons.iter().enumerate() {
        assert!(
            !reasons.iter().skip(index + 1).any(|other| other == reason),
            "`{reason}` is the reason for two refusals"
        );
    }
}

/// Every refusal answers the sender the same code and sentence, whichever
/// check it failed: telling a prober which one would say whether this daemon
/// is misconfigured.
#[test]
fn every_refusal_answers_the_sender_one_code_and_one_sentence() {
    let refused = verify_at(&keys(), DESTINATION, "", BODY).expect_err("an empty token is refused");

    assert_eq!(
        refused.code(),
        af_core::error_code::WEBHOOK_SIGNATURE_INVALID
    );
    assert_eq!(refused.detail(), afd_cron::error::detail::FIRE_UNVERIFIED);
}
