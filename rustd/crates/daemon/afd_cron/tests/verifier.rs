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
use sha2::Digest as _;

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

/// The shape check is not a decode: three base64url segments that are not a
/// token pass the shape and are then refused as undecodable, a different word
/// from a malformed one.
#[test]
fn a_token_shaped_right_that_does_not_decode_is_undecodable_not_malformed() {
    assert_eq!(
        verify_at(&keys(), DESTINATION, "aaa.bbb.ccc", BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_undecodable"
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
        "token_subject_mismatch",
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
        "token_issuer_mismatch"
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
        "token_expired",
        "leeway is zero on purpose: a tolerance is a window a replayed token \
         lives inside"
    );
}

/// The claims Upstash's own server writes, as the emulator emitted them: an
/// empty `aud`, the Go zero time (-62135596800) for `nbf`, an `iat`, and a body
/// digest that keeps its base64 padding.
fn scheduler_claims(body: &[u8]) -> serde_json::Value {
    use base64::Engine as _;
    serde_json::json!({
        "aud": "",
        "body": base64::engine::general_purpose::URL_SAFE.encode(sha2::Sha256::digest(body)),
        "exp": now() + 300,
        "iat": now(),
        "iss": "Upstash",
        "jti": "jwt_fixture0001",
        "nbf": -62_135_596_800_i64,
        "sub": DESTINATION,
    })
}

fn sign(claims: &serde_json::Value, key: &str) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(key.as_bytes()),
    )
    .expect("the claims serialise")
}

/// The regression behind a hosted schedule that never fired: the token was
/// genuine and the verifier refused it, because `nbf` was the Go zero time and
/// the body digest was padded.
#[test]
fn a_token_in_the_shape_the_scheduler_really_signs_is_believed() {
    let token = sign(&scheduler_claims(BODY), CURRENT_KEY);

    let fire = verify_at(&keys(), DESTINATION, &token, BODY)
        .expect("the scheduler's own claims are what a fire carries");

    assert_eq!(fire.message_id, "jwt_fixture0001");
}

/// Reading the digest without its padding must not make it a weaker compare: a
/// padded digest of other bytes is still somebody else's body.
#[test]
fn a_padded_digest_of_other_bytes_is_still_a_body_mismatch() {
    let token = sign(&scheduler_claims(b"something else"), CURRENT_KEY);

    assert_eq!(
        verify_at(&keys(), DESTINATION, &token, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "body_mismatch"
    );
}

/// `nbf` is not checked, because the scheduler does not set it; `exp`, the
/// subject, the issuer and the body digest still bind the token.
#[test]
fn a_start_time_in_the_future_does_not_decide_a_signed_token() {
    let mut claims = scheduler_claims(BODY);
    claims
        .as_object_mut()
        .expect("the claims are an object")
        .insert("nbf".to_owned(), serde_json::json!(now() + 600));

    verify_at(&keys(), DESTINATION, &sign(&claims, CURRENT_KEY), BODY)
        .expect("a token the scheduler signed is not refused for a start time it never set");
}

/// Every refusal answers the sender alike; only the operator's log word differs.
/// A wrong key fails the signature first, so the word comes from the key that
/// verified: a token signed by the NEXT key and expired reads as expired.
#[test]
fn the_word_comes_from_the_key_that_verified_not_the_one_that_did_not() {
    let mut claims = FireClaims::good();
    claims.exp = now() - 1;
    claims.nbf = now() - 600;

    assert_eq!(
        verify_at(&keys(), DESTINATION, &mint(&claims, NEXT_KEY), BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_expired"
    );
}

#[test]
fn a_token_missing_a_claim_the_daemon_reads_is_undecodable() {
    let claims = serde_json::json!({
        "iss": "Upstash", "sub": DESTINATION, "jti": "msg_fixture_0001",
        "exp": now() + 300, "nbf": now() - 10,
    });
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(CURRENT_KEY.as_bytes()),
    )
    .expect("the claims serialise");

    assert_eq!(
        verify_at(&keys(), DESTINATION, &token, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_undecodable",
        "no `body` claim means the signature would cover nothing this daemon checked"
    );
}

#[test]
fn a_token_signed_under_another_algorithm_is_refused_by_name() {
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS384),
        &FireClaims::good(),
        &jsonwebtoken::EncodingKey::from_secret(CURRENT_KEY.as_bytes()),
    )
    .expect("the claims serialise");

    assert_eq!(
        verify_at(&keys(), DESTINATION, &token, BODY)
            .expect_err("this token must not be believed")
            .refusal_word(),
        "token_algorithm_mismatch"
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

// ---------------------------------------------------------------------------
// Every way a token can be wrong, and every way a genuine one may differ.
//
// Tokens are built from the claims the scheduler really writes (see
// `scheduler_claims`), changing one claim at a time, so each case names the one
// thing it refuses or tolerates.
// ---------------------------------------------------------------------------

/// The scheduler's claims with `claim` set to `value`, or removed when `value`
/// is `None`.
fn changed(claim: &str, value: Option<serde_json::Value>) -> serde_json::Value {
    let mut claims = scheduler_claims(BODY);
    let object = claims.as_object_mut().expect("the claims are an object");
    match value {
        Some(value) => object.insert(claim.to_owned(), value),
        None => object.remove(claim),
    };
    claims
}

/// What `verify_at` says about `claims` signed by the current key.
fn outcome(claims: &serde_json::Value) -> Result<String, &'static str> {
    verify_at(&keys(), DESTINATION, &sign(claims, CURRENT_KEY), BODY)
        .map(|fire| fire.message_id)
        .map_err(|refused| refused.refusal_word())
}

#[test]
fn a_claim_the_scheduler_never_sets_or_sets_oddly_does_not_refuse_its_token() {
    for (claim, value) in [
        ("nbf", None),
        ("nbf", Some(serde_json::json!(0))),
        ("nbf", Some(serde_json::json!(-1))),
        ("nbf", Some(serde_json::json!(-62_135_596_800_i64))),
        ("nbf", Some(serde_json::json!(now() + 86_400))),
        ("nbf", Some(serde_json::json!("not a number"))),
        ("nbf", Some(serde_json::json!(1.5))),
        ("nbf", Some(serde_json::json!(null))),
        ("aud", None),
        ("aud", Some(serde_json::json!(""))),
        ("aud", Some(serde_json::json!("somebody"))),
        ("aud", Some(serde_json::json!(["a", "b"]))),
        ("iat", None),
        ("iat", Some(serde_json::json!(now() + 86_400))),
        ("iat", Some(serde_json::json!("yesterday"))),
        ("note", Some(serde_json::json!({"anything": [1, 2, 3]}))),
    ] {
        let claims = changed(claim, value.clone());
        assert_eq!(
            outcome(&claims),
            Ok("jwt_fixture0001".to_owned()),
            "{claim} = {value:?}"
        );
    }
}

#[test]
fn an_expiry_that_is_absent_past_or_not_a_whole_number_refuses_the_token() {
    for (value, words) in [
        (None, &["token_claims_missing"][..]),
        (Some(serde_json::json!(0)), &["token_expired"][..]),
        (Some(serde_json::json!(now() - 1)), &["token_expired"][..]),
        // A value the decoder cannot read as an unsigned whole number is either
        // reported as missing or as malformed; both refuse.
        (
            Some(serde_json::json!(-1)),
            &["token_claims_missing", "token_claim_malformed"][..],
        ),
        (
            Some(serde_json::json!("soon")),
            &["token_claims_missing", "token_claim_malformed"][..],
        ),
        (
            Some(serde_json::json!(1.5)),
            &[
                "token_claims_missing",
                "token_claim_malformed",
                "token_expired",
            ][..],
        ),
        (
            Some(serde_json::json!(null)),
            &["token_claims_missing", "token_claim_malformed"][..],
        ),
    ] {
        let refused = outcome(&changed("exp", value.clone())).expect_err("exp is enforced");
        assert!(
            words.contains(&refused),
            "exp = {value:?} read as {refused}"
        );
    }
}

#[test]
fn an_issuer_that_is_absent_or_not_exactly_the_scheduler_refuses_the_token() {
    for (value, word) in [
        (None, "token_claims_missing"),
        (Some(serde_json::json!("")), "token_issuer_mismatch"),
        (Some(serde_json::json!("upstash")), "token_issuer_mismatch"),
        (Some(serde_json::json!("Upstash ")), "token_issuer_mismatch"),
    ] {
        assert_eq!(
            outcome(&changed("iss", value.clone())),
            Err(word),
            "iss = {value:?}"
        );
    }
}

#[test]
fn a_subject_that_is_absent_or_not_exactly_this_destination_refuses_the_token() {
    for value in [
        None,
        Some(serde_json::json!("")),
        Some(serde_json::json!(format!("{DESTINATION}/"))),
        Some(serde_json::json!(format!("{DESTINATION}?x=1"))),
        Some(serde_json::json!(
            DESTINATION.replace("https://", "http://")
        )),
        Some(serde_json::json!(DESTINATION.to_uppercase())),
        Some(serde_json::json!(DESTINATION.replace("fixture", "other"))),
    ] {
        let refused = outcome(&changed("sub", value.clone()))
            .err()
            .unwrap_or("believed");
        assert!(
            ["token_claims_missing", "token_subject_mismatch"].contains(&refused),
            "sub = {value:?} read as {refused}"
        );
    }
}

#[test]
fn a_message_id_that_is_absent_empty_or_not_text_refuses_the_token() {
    for (value, word) in [
        (None, "token_undecodable"),
        (Some(serde_json::json!("")), "token_claims_missing"),
        (Some(serde_json::json!(7)), "token_undecodable"),
        (Some(serde_json::json!(null)), "token_undecodable"),
    ] {
        assert_eq!(
            outcome(&changed("jti", value.clone())),
            Err(word),
            "jti = {value:?}"
        );
    }
}

#[test]
fn a_body_digest_that_is_absent_not_text_or_not_this_body_refuses_the_token() {
    let digest = fire_token::digest_of(BODY);
    for (value, word) in [
        (None, "token_undecodable"),
        (Some(serde_json::json!(7)), "token_undecodable"),
        (Some(serde_json::json!("")), "body_mismatch"),
        (Some(serde_json::json!("=")), "body_mismatch"),
        (
            Some(serde_json::json!(format!("{digest}=="))),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(format!("{digest}==="))),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(format!("{digest} "))),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(format!(" {digest}"))),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(digest.to_uppercase())),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(&digest[..digest.len() - 1])),
            "body_mismatch",
        ),
        (
            Some(serde_json::json!(format!("{digest}A"))),
            "body_mismatch",
        ),
    ] {
        assert_eq!(
            outcome(&changed("body", value.clone())),
            Err(word),
            "body = {value:?}"
        );
    }
}

#[test]
fn the_digest_is_believed_unpadded_and_with_the_one_padding_character() {
    let digest = fire_token::digest_of(BODY);
    for value in [digest.clone(), format!("{digest}=")] {
        assert!(
            outcome(&changed("body", Some(serde_json::json!(value.clone())))).is_ok(),
            "{value}"
        );
    }
}

#[test]
fn an_algorithm_other_than_the_pinned_one_refuses_the_token() {
    for algorithm in [
        jsonwebtoken::Algorithm::HS384,
        jsonwebtoken::Algorithm::HS512,
    ] {
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(algorithm),
            &scheduler_claims(BODY),
            &jsonwebtoken::EncodingKey::from_secret(CURRENT_KEY.as_bytes()),
        )
        .expect("the claims serialise");

        assert_eq!(
            verify_at(&keys(), DESTINATION, &token, BODY)
                .expect_err("the algorithm is pinned")
                .refusal_word(),
            "token_algorithm_mismatch",
            "{algorithm:?}"
        );
    }
}

#[test]
fn a_token_with_no_signature_or_a_changed_part_refuses_the_token() {
    let token = sign(&scheduler_claims(BODY), CURRENT_KEY);
    let mut parts = token.split('.');
    let (header, payload, signature) = (
        parts.next().expect("a header"),
        parts.next().expect("a payload"),
        parts.next().expect("a signature"),
    );
    let flipped = |part: &str| {
        let mut chars: Vec<char> = part.chars().collect();
        if let Some(last) = chars.last_mut() {
            *last = if *last == 'A' { 'B' } else { 'A' };
        }
        chars.into_iter().collect::<String>()
    };

    for (label, tampered) in [
        (
            "signature changed",
            format!("{header}.{payload}.{}", flipped(signature)),
        ),
        (
            "payload changed",
            format!("{header}.{}.{signature}", flipped(payload)),
        ),
        (
            "header changed",
            format!("{}.{payload}.{signature}", flipped(header)),
        ),
        ("signature dropped", format!("{header}.{payload}.")),
        (
            "alg none, unsigned",
            format!("{}.{payload}.", "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0"),
        ),
    ] {
        let refused = verify_at(&keys(), DESTINATION, &tampered, BODY)
            .expect_err("a token that was altered must not be believed")
            .refusal_word();
        assert!(
            [
                "signature_invalid",
                "token_malformed",
                "token_undecodable",
                "token_algorithm_mismatch"
            ]
            .contains(&refused),
            "{label} read as {refused}"
        );
    }
}

#[test]
fn either_key_verifies_and_a_blank_one_is_skipped() {
    let claims = scheduler_claims(BODY);
    let blank = || af_crypto::secret::SecretString::new(String::new());
    let secret = |text: &str| af_crypto::secret::SecretString::new(text.to_owned());

    for (label, held, signed_with, believed) in [
        ("current", keys(), CURRENT_KEY, true),
        ("next", keys(), NEXT_KEY, true),
        ("neither", keys(), "a-key-nobody-holds", false),
        (
            "only next held",
            SigningKeys {
                current: blank(),
                next: secret(NEXT_KEY),
            },
            NEXT_KEY,
            true,
        ),
        (
            "only current held",
            SigningKeys {
                current: secret(CURRENT_KEY),
                next: blank(),
            },
            NEXT_KEY,
            false,
        ),
    ] {
        let verdict = verify_at(&held, DESTINATION, &sign(&claims, signed_with), BODY);
        assert_eq!(verdict.is_ok(), believed, "{label}");
    }
}
