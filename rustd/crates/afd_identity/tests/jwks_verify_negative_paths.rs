//! Dimension 4.2 — bad signature, expired, wrong audience and wrong issuer each
//! refuse, and a key-id miss triggers EXACTLY ONE refresh.
//!
//! Every token here is one the Zig daemon signs and verifies. The fixtures are
//! copied from the retired daemon's `auth/jwks_test_fixtures.zig` and pinned to it
//! by `test_the_fixtures_are_the_zig_daemons`, so these are real RS256
//! signatures over a real 2048-bit key — not a Rust-side key pair that would
//! only prove this implementation agrees with itself.
//!
//! Nothing here opens a socket or reads a wall clock. The key set arrives
//! through [`StaticKeySet`], which counts its reads, and time arrives through
//! `FixedClock`, so "exactly one refresh" and "expired" are both assertions on
//! a number rather than inferences from a trace.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_auth::credential::Presented;
use afd_auth::verifier::{TokenVerifier, VerifyError};
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::jwks::source::KeySetSource;
use afd_identity::{JwksVerifier, StaticKeySet, VerifierConfig, jwks_url};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

// ── The Zig daemon's fixtures, pinned below ──────────────────────────────

/// Base64url modulus of the shared 2048-bit RSA test key.
const TEST_RSA_N: &str = "7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ";
const TEST_KID: &str = "test-kid-static";

/// JWT `exp` and `iat` are seconds; the clock takes milliseconds.
const MILLIS_PER_SECOND: i64 = 1_000;
const TEST_HEADER: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID: &str = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ";
const TEST_PAYLOAD_EXPIRED: &str = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjoxNzA0MDY3MzAwfQ";
const TEST_SIG_VALID: &str = "pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";
const TEST_SIG_EXPIRED: &str = "Ctiud6VRvF54eited-UOq6HEiKZWNdhPli_w_rsFLmS6bmeDr2xuXlag6HgZLCnOc1mHsoJGGqeojZ8xt2SVt6JHjxXxV6KhP6orw4FPgmPKzyZw2zFWGmi3M0IUSv9CzsaaWnoj5KdLL9DWzF--EpMDddqaEMBLolxuMV_uO0m6Fji6lJikVZaPTFJ0YMzcMdkvh4iZ9_y2fGYvjUSPnbNw-3eq4P4tlUq2n_6ES17zLGIF55cRoUo7v-audITTd9AVwp0Y3-_PXq-yAJEPTBhysG1iYiKMrf9x_r1h6g2rKCFQ_7k48K-o_rUPSFVaU0Q3TXG3CDoMjmAma0LN6A";

const ISSUER: &str = "https://clerk.dev.agentsfleet.net";
const AUDIENCE: &str = "https://api.agentsfleet.net";

/// The fixture tokens' `iat`. Every clock below is expressed relative to it.
const FIXTURE_IAT_S: i64 = 1_704_067_200;
/// `TEST_PAYLOAD_EXPIRED`'s `exp` — one hundred seconds after `iat`.
const EXPIRED_EXP_S: i64 = 1_704_067_300;

/// A moment AFTER the expiring token's expiry, and long before the valid one's.
///
/// One clock makes both assertions: the valid token verifies here, the expiring
/// one does not, and the only difference between them is `exp`.
fn now_after_expiry() -> Arc<dyn Clock> {
    Arc::new(FixedClock::at(UnixMillis::from_millis(
        (EXPIRED_EXP_S + 100) * MILLIS_PER_SECOND,
    )))
}

/// A moment BEFORE the expiring token's expiry.
fn now_before_expiry() -> Arc<dyn Clock> {
    Arc::new(FixedClock::at(UnixMillis::from_millis(
        (FIXTURE_IAT_S + 10) * MILLIS_PER_SECOND,
    )))
}

/// A one-key key set over the shared modulus, published under `kid`.
///
/// The single spelling of the envelope, exactly as `jwks_test_fixtures.zig`
/// builds it with `rsaKeySet` (RULE UFS).
fn key_set(kid: &str) -> String {
    format!(
        "{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{kid}\",\"use\":\"sig\",\"alg\":\"RS256\",\"n\":\"{TEST_RSA_N}\",\"e\":\"AQAB\"}}]}}"
    )
}

fn token(header: &str, payload: &str, signature: &str) -> Presented {
    Presented::new(&format!("{header}.{payload}.{signature}")).expect("a non-blank token")
}

fn valid_token() -> Presented {
    token(TEST_HEADER, TEST_PAYLOAD_VALID, TEST_SIG_VALID)
}

fn verifier(document: String, clock: Arc<dyn Clock>) -> JwksVerifier<StaticKeySet> {
    JwksVerifier::new(
        StaticKeySet::new(document.into_bytes()),
        VerifierConfig::new(ISSUER, AUDIENCE),
        clock,
    )
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

// ── The happy path, so the negatives mean something ──────────────────────

/// A real signature over a real key verifies, and the claims arrive.
///
/// Without this every refusal below could be passing for the wrong reason.
#[test]
fn test_a_valid_token_verifies_against_the_published_key_set() {
    let verifier = verifier(key_set(TEST_KID), now_after_expiry());
    let claims = block_on(verifier.verify(&valid_token())).expect("the fixture token is valid");

    assert_eq!(claims.subject.as_str(), "user_test");
    // The fixture carries no `scopes` claim, which is the fail-closed default.
    assert!(claims.scope_claim.is_none());
    // `tenant_a` is not a version-7 identifier, so the tenant reads as absent
    // rather than failing the whole verification — a provisioning problem is
    // not evidence about the token.
    assert!(claims.tenant.is_none());
    assert_eq!(verifier.source().fetches(), 1, "one fetch to prime the set");
}

// ── The four refusals the dimension names ────────────────────────────────

/// Expiry is decided by the injected clock and nothing else.
///
/// The same token, the same key, the same signature — verified at one instant
/// and refused at another. That is what makes this a test of the expiry check
/// rather than of whatever else might have gone wrong.
#[test]
fn test_expiry_is_decided_by_the_clock_and_not_by_the_signature() {
    let expiring = token(TEST_HEADER, TEST_PAYLOAD_EXPIRED, TEST_SIG_EXPIRED);

    let before = verifier(key_set(TEST_KID), now_before_expiry());
    block_on(before.verify(&expiring)).expect("not yet expired");

    let after = verifier(key_set(TEST_KID), now_after_expiry());
    let refused = block_on(after.verify(&expiring)).expect_err("now expired");
    assert_eq!(refused, VerifyError::Expired);
}

/// A token expiring exactly now is expired.
///
/// `jwks_standard_claims.zig` writes `if (exp <= now_s)`, and the boundary is
/// worth pinning: `<` instead of `<=` would honour a token for one more second
/// than the issuer said, on every request, forever.
#[test]
fn test_a_token_expiring_exactly_now_is_expired() {
    let at_the_boundary = Arc::new(FixedClock::at(UnixMillis::from_millis(
        EXPIRED_EXP_S * MILLIS_PER_SECOND,
    )));
    let verifier = verifier(key_set(TEST_KID), at_the_boundary);
    let refused =
        block_on(verifier.verify(&token(TEST_HEADER, TEST_PAYLOAD_EXPIRED, TEST_SIG_EXPIRED)))
            .expect_err("exp <= now is expired");
    assert_eq!(refused, VerifyError::Expired);
}

/// A tampered signature does not verify.
#[test]
fn test_a_tampered_signature_is_refused() {
    let verifier = verifier(key_set(TEST_KID), now_after_expiry());

    // One character changed, in a segment that still decodes.
    let mut tampered = TEST_SIG_VALID.to_owned();
    tampered.replace_range(0..1, "q");
    let refused = block_on(verifier.verify(&token(TEST_HEADER, TEST_PAYLOAD_VALID, &tampered)))
        .expect_err("a changed signature must not verify");
    assert_eq!(refused, VerifyError::SignatureInvalid);

    // And a payload swapped under a signature made for a different one — the
    // attack the signing input exists to prevent.
    let swapped =
        block_on(verifier.verify(&token(TEST_HEADER, TEST_PAYLOAD_EXPIRED, TEST_SIG_VALID)))
            .expect_err("a signature is over this payload, not any payload");
    assert_eq!(swapped, VerifyError::SignatureInvalid);
}

/// A token minted for a sibling service is refused here.
///
/// The property `docs/AUTH.md` §Per-microservice JWT templates rests on: a
/// leaked token cannot be replayed against another service because each checks
/// only its own audience, in the verifier rather than in application logic.
#[test]
fn test_a_token_for_another_audience_is_refused() {
    let verifier = JwksVerifier::new(
        StaticKeySet::new(key_set(TEST_KID).into_bytes()),
        VerifierConfig::new(ISSUER, "https://storage.agentsfleet.net"),
        now_after_expiry(),
    );
    let refused =
        block_on(verifier.verify(&valid_token())).expect_err("minted for a different service");
    assert_eq!(refused, VerifyError::AudienceMismatch);
}

/// A token from another issuer is refused, even signed by a key we hold.
///
/// The signature verifying proves the KEY, not the issuer, so the `iss` check
/// is the only thing standing between a valid signature and a foreign identity.
#[test]
fn test_a_token_from_another_issuer_is_refused() {
    let verifier = JwksVerifier::new(
        StaticKeySet::new(key_set(TEST_KID).into_bytes()),
        VerifierConfig::new("https://clerk.other.example", AUDIENCE),
        now_after_expiry(),
    );
    let refused = block_on(verifier.verify(&valid_token())).expect_err("a different issuer");
    assert_eq!(refused, VerifyError::IssuerMismatch);
}

// ── The refresh policy ───────────────────────────────────────────────────

/// A key-id miss triggers exactly one refresh — the dimension's own words.
///
/// The set is fresh and simply does not carry the token's key, which is what a
/// rotation looks like from here. Repeating the request must not repeat the
/// fetch: the rate limit is what stops a key nobody published from becoming a
/// request-rate hammer on the identity provider.
#[test]
fn test_a_key_id_miss_triggers_exactly_one_refresh() {
    // Published under the WRONG kid, so the token's key is never in the set.
    let verifier = verifier(key_set("wrong-kid"), now_after_expiry());

    let first = block_on(verifier.verify(&valid_token())).expect_err("the key is not published");
    assert_eq!(first, VerifyError::KeyNotFound);
    let after_first = verifier.source().fetches();
    assert_eq!(after_first, 1, "priming the empty cache is the one fetch");

    for _ in 0..5 {
        let refused = block_on(verifier.verify(&valid_token())).expect_err("still not published");
        assert_eq!(refused, VerifyError::KeyNotFound);
    }
    assert_eq!(
        verifier.source().fetches(),
        after_first,
        "a miss on a fresh set must not re-fetch inside the rate-limit window"
    );
}

/// A failed refresh serves the key set we already hold.
///
/// Emptying the cache on a fetch failure would turn a provider blip into every
/// token failing at once, which is strictly worse than keys that are stale.
#[test]
fn test_a_failed_refresh_serves_the_previously_held_key_set() {
    /// Answers once, then fails — a provider that goes away after boot.
    #[derive(Debug)]
    struct FailsAfterFirst {
        document: Vec<u8>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl KeySetSource for FailsAfterFirst {
        fn fetch(&self) -> impl Future<Output = Result<Vec<u8>, VerifyError>> + Send {
            let seen = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::future::ready(if seen == 0 {
                Ok(self.document.clone())
            } else {
                Err(VerifyError::KeySetUnavailable)
            })
        }
    }

    // A time-to-live of zero makes every read see an expired set, so every
    // verification attempts a refresh — deterministic where a sleep would be
    // flaky.
    let verifier = JwksVerifier::new(
        FailsAfterFirst {
            document: key_set(TEST_KID).into_bytes(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        },
        VerifierConfig {
            issuer: ISSUER.into(),
            audience: AUDIENCE.into(),
            ttl_ms: 0,
        },
        now_after_expiry(),
    );

    block_on(verifier.verify(&valid_token())).expect("the first fetch succeeds");
    block_on(verifier.verify(&valid_token()))
        .expect("the provider is gone and the held key set still verifies");
}

#[path = "jwks_verify_negative_paths/header_and_keys.rs"]
mod header_and_keys;
