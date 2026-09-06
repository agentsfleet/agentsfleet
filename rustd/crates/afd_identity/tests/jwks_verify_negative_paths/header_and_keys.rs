//! Header restrictions, key suitability and endpoint selection.
use super::*;

// ── Header handling, before any key is involved ──────────────────────────

/// Only `RS256` is accepted, and `none` is refused by that same rule.
///
/// An allowlist of one rather than a blocklist of the dangerous ones: the
/// algorithm-confusion attack is unreachable by construction instead of by
/// remembering to exclude it.
#[test]
fn test_only_rs256_is_accepted() {
    let verifier = verifier(key_set(TEST_KID), now_after_expiry());
    for alg in ["none", "HS256", "RS384", "ES256", ""] {
        let header = URL_SAFE_NO_PAD.encode(format!(
            "{{\"alg\":\"{alg}\",\"typ\":\"JWT\",\"kid\":\"{TEST_KID}\"}}"
        ));
        let refused =
            block_on(verifier.verify(&token(&header, TEST_PAYLOAD_VALID, TEST_SIG_VALID)))
                .expect_err("only RS256 is accepted");
        assert_eq!(refused, VerifyError::UnsupportedAlgorithm, "alg={alg}");
    }
}

/// A header with no `kid` selects no key, and says so.
#[test]
fn test_a_header_without_a_key_id_is_refused() {
    let verifier = verifier(key_set(TEST_KID), now_after_expiry());
    let header = URL_SAFE_NO_PAD.encode("{\"alg\":\"RS256\",\"typ\":\"JWT\"}");
    let refused = block_on(verifier.verify(&token(&header, TEST_PAYLOAD_VALID, TEST_SIG_VALID)))
        .expect_err("no key id, no key");
    assert_eq!(refused, VerifyError::MissingKeyId);
}

/// Everything that is not three decodable segments is malformed.
#[test]
fn test_a_malformed_token_is_refused_before_any_key_is_read() {
    let verifier = verifier(key_set(TEST_KID), now_after_expiry());
    for bad in [
        "not-a-jwt",
        "only.two",
        "a.b.c.d",
        "..",
        ".b.c",
        "a..c",
        "a.b.",
        "!!!.b.c",
    ] {
        let refused = block_on(verifier.verify(&Presented::new(bad).expect("non-blank")))
            .expect_err("malformed");
        assert_eq!(refused, VerifyError::Malformed, "{bad:?}");
    }
    assert_eq!(
        verifier.source().fetches(),
        0,
        "a malformed token must not cost a fetch"
    );
}

// ── The recorded divergence, made visible ────────────────────────────────

/// A 1024-bit key is refused, and refused LOUDLY at parse rather than silently
/// at verification.
///
/// `jwks_crypto.zig` accepts moduli from 1024 bits up; this daemon verifies
/// with `RSA_PKCS1_2048_8192_SHA256`, whose floor is 2048. The divergence is
/// recorded in the milestone spec. What this pins is that it cannot fail
/// quietly: a key set carrying only such a key is `KeySetUnavailable`, which
/// `prime()` turns into a boot refusal — not a 401 on every session token while
/// `agt_t` and `afc_` keep working.
#[test]
fn test_a_key_below_the_verifiable_floor_is_refused_at_parse() {
    let short_modulus = URL_SAFE_NO_PAD.encode([0xAB_u8; 128]); // 1024 bits
    let document = format!(
        "{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{TEST_KID}\",\"n\":\"{short_modulus}\",\"e\":\"AQAB\"}}]}}"
    );
    let verifier = verifier(document, now_after_expiry());

    let refused = block_on(verifier.prime()).expect_err("boot must refuse a key set it cannot use");
    assert_eq!(refused, VerifyError::KeySetUnavailable);
}

/// A key set that publishes an unusable key BESIDE a usable one still works.
///
/// Refusing the whole document over a key we were never going to use would be
/// an outage authored by strictness — providers legitimately publish keys for
/// other algorithms alongside the signing one.
#[test]
fn test_an_unusable_key_beside_a_usable_one_is_skipped() {
    let document = format!(
        "{{\"keys\":[{{\"kty\":\"EC\",\"kid\":\"ec-key\",\"crv\":\"P-256\"}},\
         {{\"kty\":\"RSA\",\"kid\":\"{TEST_KID}\",\"n\":\"{TEST_RSA_N}\",\"e\":\"AQAB\"}}]}}"
    );
    let verifier = verifier(document, now_after_expiry());
    block_on(verifier.verify(&valid_token())).expect("the usable key still verifies");
}

// ── Configuration ────────────────────────────────────────────────────────

/// One resolver decides the endpoint, so a doctor command and the daemon can
/// never test a different URL than the one that gets fetched.
#[test]
fn test_the_key_set_url_is_derived_from_the_issuer() {
    assert_eq!(
        jwks_url(None, Some(ISSUER)).as_deref(),
        Some("https://clerk.dev.agentsfleet.net/.well-known/jwks.json")
    );
    // Every trailing slash is stripped: a doubled slash in the path 404s.
    assert_eq!(
        jwks_url(None, Some("https://issuer.example///")).as_deref(),
        Some("https://issuer.example/.well-known/jwks.json")
    );
    // A padded value in an environment file is a typo, not a dead URL.
    assert_eq!(
        jwks_url(None, Some("  https://issuer.example \n")).as_deref(),
        Some("https://issuer.example/.well-known/jwks.json")
    );
    // An explicit override wins, verbatim once trimmed.
    assert_eq!(
        jwks_url(Some(" https://keys.example/keys "), Some(ISSUER)).as_deref(),
        Some("https://keys.example/keys")
    );
    // An empty override does not shadow a usable issuer.
    assert_eq!(
        jwks_url(Some("   "), Some(ISSUER)).as_deref(),
        Some("https://clerk.dev.agentsfleet.net/.well-known/jwks.json")
    );
    // Neither: the deployment has no identity provider.
    assert_eq!(jwks_url(None, None), None);
    assert_eq!(jwks_url(Some(""), Some("  ")), None);
}

// ── Parity ───────────────────────────────────────────────────────────────

/// The shared key really is 2048-bit, so the floor this daemon enforces is not
/// being dodged by the fixture that proves the happy path.
///
/// This stood beside a byte-for-byte comparison against
/// the retired daemon's `auth/jwks_test_fixtures.zig`, which the tree's deletion
/// takes with it: once there is no second implementation, "my fixtures equal
/// theirs" has nothing to compare against and freezing it would assert a
/// constant against itself. The fixtures above ARE those bytes, copied while
/// the tree stood and recorded as such in this file's header; what survives is
/// the property that made them worth sharing.
#[test]
fn test_the_shared_key_meets_the_modulus_floor() {
    let modulus = URL_SAFE_NO_PAD
        .decode(TEST_RSA_N)
        .expect("the fixture modulus decodes");
    assert_eq!(modulus.len(), 256, "the shared test key is 2048-bit");
}
