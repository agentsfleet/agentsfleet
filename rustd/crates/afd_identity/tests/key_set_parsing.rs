//! What this daemon accepts of a published key set, and what it reports.
//!
//! Separate from the verification suite because these are decisions made at
//! PARSE time, before any token exists — and because the reason they are made
//! at parse time is so a boot check can act on them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_auth::verifier::VerifyError;
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::jwks::key_set::{MAX_MODULUS_BYTES, MIN_MODULUS_BYTES};
use afd_identity::{JwkKeySet, JwksVerifier, StaticKeySet, VerifierConfig};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

const TEST_RSA_N: &str = "7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ";
const TEST_KID: &str = "test-kid-static";
const ISSUER: &str = "https://clerk.dev.agentsfleet.net";
const AUDIENCE: &str = "https://api.agentsfleet.net";

fn rsa_key(kid: &str, n: &str) -> String {
    format!(
        "{{\"kty\":\"RSA\",\"kid\":\"{kid}\",\"use\":\"sig\",\"alg\":\"RS256\",\"n\":\"{n}\",\"e\":\"AQAB\"}}"
    )
}

fn document(keys: &[String]) -> String {
    format!("{{\"keys\":[{}]}}", keys.join(","))
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::at(UnixMillis::from_millis(1_704_067_400_000)))
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// A usable set reports its keys, and each key reports its own identifier.
#[test]
fn test_a_parsed_set_reports_what_it_holds() {
    let set = JwkKeySet::parse(document(&[rsa_key(TEST_KID, TEST_RSA_N)]).as_bytes())
        .expect("a usable key set");

    assert_eq!(set.len(), 1);
    assert!(!set.is_empty());
    assert!(set.usable(), "boot may proceed");
    assert_eq!(set.rejected(), 0);

    let key = set.find(TEST_KID).expect("the published key");
    assert_eq!(key.kid(), TEST_KID);
    assert_eq!(key.modulus().len(), MIN_MODULUS_BYTES, "a 2048-bit key");
    assert_eq!(key.exponent(), &[0x01, 0x00, 0x01], "AQAB is 65537");
    assert!(set.find("never-published").is_none());
}

/// Keys this daemon cannot use are counted, not hidden.
///
/// "The issuer published three keys and this daemon can use one" is a different
/// operator problem from "the issuer published one key", and a boot log that
/// cannot tell them apart sends someone looking in the wrong place.
#[test]
fn test_unusable_keys_are_counted_rather_than_hidden() {
    let too_short = URL_SAFE_NO_PAD.encode([0xAB_u8; MIN_MODULUS_BYTES - 1]);
    let too_long = URL_SAFE_NO_PAD.encode(vec![0xAB_u8; MAX_MODULUS_BYTES + 1]);
    let set = JwkKeySet::parse(
        document(&[
            // Another algorithm entirely — legitimate to publish beside ours.
            "{\"kty\":\"EC\",\"kid\":\"ec\",\"crv\":\"P-256\"}".to_owned(),
            // Below and above what `RSA_PKCS1_2048_8192_SHA256` verifies.
            rsa_key("short", &too_short),
            rsa_key("long", &too_long),
            // No `kid`, so nothing could ever select it.
            "{\"kty\":\"RSA\",\"n\":\"AQAB\",\"e\":\"AQAB\"}".to_owned(),
            // A modulus that is not base64url at all.
            rsa_key("garbage", "!!!not-base64!!!"),
            // The one we can use.
            rsa_key(TEST_KID, TEST_RSA_N),
        ])
        .as_bytes(),
    )
    .expect("one usable key is enough");

    assert_eq!(set.len(), 1);
    assert_eq!(set.rejected(), 5);
    assert!(set.usable());
}

/// A key set with nothing usable is unavailable, not empty.
///
/// The distinction boot acts on: an empty set that verified nothing would
/// answer 401 to every session token while `agt_t` and `afc_` kept working.
#[test]
fn test_a_set_with_nothing_usable_is_unavailable() {
    for raw in [
        // Not JSON.
        "not json".to_owned(),
        // JSON, but not a key set.
        "{}".to_owned(),
        // A key set with no keys.
        document(&[]),
        // Keys, none of them usable.
        document(&["{\"kty\":\"EC\",\"kid\":\"ec\"}".to_owned()]),
    ] {
        assert_eq!(
            JwkKeySet::parse(raw.as_bytes()).expect_err("nothing to verify with"),
            VerifyError::KeySetUnavailable,
            "{raw}"
        );
    }
}

/// Priming succeeds on a usable set — the boot check §7 calls.
#[test]
fn test_priming_succeeds_on_a_usable_key_set() {
    let verifier = JwksVerifier::new(
        StaticKeySet::new(document(&[rsa_key(TEST_KID, TEST_RSA_N)]).into_bytes()),
        VerifierConfig::new(ISSUER, AUDIENCE),
        clock(),
    );
    block_on(verifier.prime()).expect("boot may proceed");
    assert_eq!(verifier.source().fetches(), 1);
}
