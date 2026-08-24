//! Which claim shapes this daemon reads, proved on tokens it actually signed.
//!
//! The `jwks_test_fixtures.zig` tokens verify a real signature over a real key,
//! which is what makes the negative paths in `jwks_verify_negative_paths`
//! meaningful — but they carry ONE payload, so no fixture can say what happens
//! to an array audience, or to a tenant claim at the top level rather than
//! nested. Reading claims off an unsigned token would prove nothing, because
//! this verifier refuses to read claims for a decision before the signature
//! verifies (which is itself a property, pinned over there).
//!
//! So these tests sign. The key is the Zig tree's own throwaway
//! `TEST_KEY_PKCS1_B64` from `auth/crypto/rs256_sign.zig` — generated offline,
//! never used in production, and already embedded there to drive the signer.
//! `ring` signs as well as verifies, so this needs no crate the library does
//! not already link.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_auth::credential::Presented;
use afd_auth::scope::{Scope, parse_claim};
use afd_auth::verifier::{TokenVerifier, VerifiedClaims, VerifyError};
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::{JwksVerifier, StaticKeySet, VerifierConfig};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

/// A throwaway 2048-bit RSA key as PKCS#1 DER, base64 without PEM armor.
///
/// `auth/crypto/rs256_sign.zig`'s `TEST_KEY_PKCS1_B64`, pinned to it below.
const TEST_KEY_PKCS1_B64: &str = "MIIEowIBAAKCAQEAxBs4iJWrDhpuy4GQyfrQhtnrXhzEM86cswmwrs9ouW5S4cCi+yzb+xsMZrK2n1AkVkep6c56My6P/13awSMYdtejrSs/b71W+iE83XSWPJJI4sjzUJ0UEU/AQMiMW6LVmWU55n25NyhVOrLxqO3DI5Kb6qlCxDL1yXgyKEmls1e0qXQD2kigsJp6QhcxXgPhAX6wUL0nhSUACPFG468iRU3DLR66dAsTcy7FjNWxh8ljC8ScM9Rm6yNo9i9CGTQQIRwAolMpIMcSxpBKEIhZpwkiEgtwkSvI1s+u5GxSZ6IyBM9tooyb1TlRsWhYm9pkrroGeG0Y3YSdZawXOWrEUwIDAQABAoIBAFC5J8dJXJU8mjjZB6GsxeOMlo8x5i2xMd2c8oayx9f0qtdUtYIREChIFQ29KOFhWuPNMgsVPEYPN6UVnDN+X9ajozNoJv+2/7OMtQIvuJwMV0ZLE6UuU5Fgs7G3G9eoqqYu/et7+x7SUmsMN9+ip33gHqA0tlAO7g/Vk0f0MOomSYGg85ClU9tUVqWS9WOZk7dDcF0zmXDG2aoZEE0oSV62ysQqtkX6ClC2XX4ZtiaBrPGEMB5yxNr6uPiHj7p0IAJtpxRa7jJ5ylWMYqqYGVsGBRkxYsFIDfXs79oxrs9Jf93wZ7A/yyhgWgU9B05LiO8jZ29VlMyu2BqgvP6ITzECgYEA+0UWu8O9vKEMOq2w8AZh9rDQL/L/mJNVFwKok6j6uBQgdvgN5M/ga8tfXj+PR8slFBhydDj80lESxNWwgTzcn7bNglSxdV4A+gCa01o5W6XE0mSe/hug+7pIR2wO9UYNT0gh10Av0xyUn62dLq2qBT60D0HzX57x5Axv6+Ua3TkCgYEAx8xKLWD18oavkQKucVXR/vTb8OWX6qrKG6IFEtzxOAyaRXN7y/cB7rJdl91ytTvZ4djc3lz+Zj9n3DU3HTtj85MktyomawKNpSif4BMx1MzS7cMX24y8ixBzHhroCObu4h200AIWEs3/4HhafTBVLj8tY65WiPfvqrYQPuKAOesCgYEAh3K2zoC1xvkJnpgCyWCnblPh5fcX0Seatsy4EuEERjaTSY5t7uogD/uRbTzV/92CH1MOX5hYsQcDFxgaDZDBXVctcRQ2lQ4XeKzayRPZ142Ei+Wxz0kVfpzsWZPmfFFG23YGyAHRxfuiInF0SbVT8X/bkF38047a1hPeQUs/MAECgYBQLQebPCyWHTw4ycWsz06MrD/SZJ/Y2J5wBk1Y63aVEmGZ+ySzjbSlz8fFGGVemtztR3Qie1jPOSR5dpVeUqXiaaqzIeP2zzh+DVZSugEmLud55+8b+Fb0yy4W558za1BzRo53Zk7rTuUec82ELTARdeLF/IDXR/9SFutgAM6J7wKBgF/WfKsWeV++aRYXS7vsJqq9xM+P1y9JNcIUtItVA7eYe9vbm8/mQ5e1Qln45k1EgzzkcYBBVbuTF5d92xMAHLfdZUjRCDMc752b9B6i1pgPUnd8w1YDoYK7V/wVavOhXuNPc+btdItLFps0+eOa2NCmJ7G4ekqIAvrTRwmwKlJa";

const KID: &str = "signing-fixture";
const ISSUER: &str = "https://clerk.dev.agentsfleet.net";
const AUDIENCE: &str = "https://api.agentsfleet.net";
/// A valid version-7 identifier — the shape a tenant claim must have.
const TENANT: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
const WORKSPACE: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
/// Far in the future, so nothing here is about expiry.
const NOT_EXPIRED: i64 = 4_102_444_800;

fn key_pair() -> ring::signature::RsaKeyPair {
    let der = STANDARD
        .decode(TEST_KEY_PKCS1_B64)
        .expect("the fixture key is base64");
    ring::signature::RsaKeyPair::from_der(&der).expect("a PKCS#1 RSA private key")
}

/// A key set publishing the signing key's own public components.
fn key_set() -> String {
    let pair = key_pair();
    let components: ring::signature::RsaPublicKeyComponents<Vec<u8>> = pair.public().into();
    let n = URL_SAFE_NO_PAD.encode(&components.n);
    let e = URL_SAFE_NO_PAD.encode(&components.e);
    format!("{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{KID}\",\"n\":\"{n}\",\"e\":\"{e}\"}}]}}")
}

/// Signs `payload` into a compact RS256 token.
fn sign(payload: &str) -> Presented {
    let header = URL_SAFE_NO_PAD.encode(format!(
        "{{\"alg\":\"RS256\",\"typ\":\"JWT\",\"kid\":\"{KID}\"}}"
    ));
    let body = URL_SAFE_NO_PAD.encode(payload);
    let signing_input = format!("{header}.{body}");

    let pair = key_pair();
    let mut signature = vec![0_u8; pair.public().modulus_len()];
    pair.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        signing_input.as_bytes(),
        &mut signature,
    )
    .expect("signing the fixture key");

    Presented::new(&format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(&signature)
    ))
    .expect("a non-blank token")
}

fn verify(payload: &str) -> Result<VerifiedClaims, VerifyError> {
    let verifier = JwksVerifier::new(
        StaticKeySet::new(key_set().into_bytes()),
        VerifierConfig::new(ISSUER, AUDIENCE),
        Arc::new(FixedClock::at(UnixMillis::from_millis(1_704_067_400_000))) as Arc<dyn Clock>,
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(verifier.verify(&sign(payload)))
}

/// Signing works, so a failure below means the claim shape and nothing else.
#[test]
fn test_the_signing_fixture_produces_a_token_this_daemon_verifies() {
    let claims = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\"exp\":{NOT_EXPIRED}}}"
    ))
    .expect("a token this daemon signed and published a key for");
    assert_eq!(claims.subject.as_str(), "user_x");
}

/// The tenant claim is read from `metadata`, which is where it actually is.
///
/// `clerk_metadata_payload.zig` writes two keys into `public_metadata`, and the
/// session-token template projects `metadata.tenant_id`. A reader that looked
/// only at the top level would find the tenant on NO production token — every
/// dashboard session would authenticate and then be refused for having no
/// tenant. This is the test for that.
#[test]
fn test_the_tenant_claim_is_read_from_the_nested_metadata_object() {
    let claims = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
          \"exp\":{NOT_EXPIRED},\"metadata\":{{\"tenant_id\":\"{TENANT}\",\
          \"workspace_id\":\"{WORKSPACE}\"}}}}"
    ))
    .expect("a valid token");

    assert_eq!(claims.tenant.expect("nested tenant").as_str(), TENANT);
    assert_eq!(
        claims.workspace_scope.expect("nested ceiling").as_str(),
        WORKSPACE
    );
}

/// A top-level projection wins over the nested one.
///
/// The order `claims.zig::getClerkTenantId` walks, and it is the order that
/// lets a template start projecting to the top level without both readers
/// having to change at once.
#[test]
fn test_a_top_level_tenant_claim_wins_over_the_nested_one() {
    let claims = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
          \"exp\":{NOT_EXPIRED},\"tenant_id\":\"{TENANT}\",\
          \"metadata\":{{\"tenant_id\":\"{WORKSPACE}\"}}}}"
    ))
    .expect("a valid token");
    assert_eq!(claims.tenant.expect("top-level tenant").as_str(), TENANT);
}

/// An array audience is matched strictly — the shape half of providers send.
#[test]
fn test_an_array_audience_is_matched_strictly() {
    let claims = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\
          \"aud\":[\"https://other.example\",\"{AUDIENCE}\"],\"exp\":{NOT_EXPIRED}}}"
    ))
    .expect("our audience is in the array");
    assert_eq!(claims.subject.as_str(), "user_x");

    // And an array that does NOT name us is refused, so the array form is not
    // an accidental way past a check the string form enforces.
    assert_eq!(
        verify(&format!(
            "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\
              \"aud\":[\"https://other.example\",\"https://third.example\"],\
              \"exp\":{NOT_EXPIRED}}}"
        ))
        .expect_err("minted for other services"),
        VerifyError::AudienceMismatch
    );
}

/// Every shape of a missing or unusable audience claim is refused.
#[test]
fn test_an_absent_or_unusable_audience_is_refused() {
    for aud in [
        "",
        "\"aud\":null,",
        "\"aud\":[],",
        "\"aud\":123,",
        "\"aud\":{},",
    ] {
        let payload =
            format!("{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",{aud}\"exp\":{NOT_EXPIRED}}}");
        assert_eq!(
            verify(&payload).expect_err("no usable audience"),
            VerifyError::AudienceMismatch,
            "{aud}"
        );
    }
}

/// A token with no `exp`, no `sub`, or a blank `sub` is refused.
///
/// Not `Malformed`: the token was well formed and validly signed. It simply
/// does not carry what a principal needs, and saying so distinctly is what
/// tells an operator to look at their claim template rather than their client.
#[test]
fn test_a_token_missing_a_required_claim_is_refused() {
    for payload in [
        // No expiry — a token that never expires is not one this daemon honours.
        format!("{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\"}}"),
        // No subject.
        format!("{{\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\"exp\":{NOT_EXPIRED}}}"),
        // A subject that carries no identity.
        format!(
            "{{\"sub\":\"   \",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\"exp\":{NOT_EXPIRED}}}"
        ),
    ] {
        assert_eq!(
            verify(&payload).expect_err("incomplete"),
            VerifyError::MissingClaim,
            "{payload}"
        );
    }
}

/// A payload that is not a JSON object is malformed, whatever it is signed with.
#[test]
fn test_a_non_object_payload_is_malformed() {
    for payload in ["[]", "\"a string\"", "42", "null"] {
        assert_eq!(
            verify(payload).expect_err("not a claim set"),
            VerifyError::Malformed,
            "{payload}"
        );
    }
}

/// The capability claim is read at the top level, and nowhere else.
///
/// `claims.zig` records why: an earlier ladder tried `OAuth2`'s `scope` BEFORE
/// this one, so a token carrying a standard `scope` claim would silently have
/// supplied a different capability set on the authorisation path.
#[test]
fn test_the_capability_claim_is_read_from_one_place_only() {
    let held = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
          \"exp\":{NOT_EXPIRED},\"scopes\":\"fleet:admin\"}}"
    ))
    .expect("a valid token")
    .scope_claim
    .expect("the claim is present");
    assert_eq!(parse_claim(&held), parse_claim(Scope::FleetAdmin.wire()));

    // A standard `scope` claim, and a nested one, grant nothing.
    for decoy in [
        "\"scope\":\"fleet:admin\"",
        "\"metadata\":{\"scopes\":\"fleet:admin\"}",
    ] {
        let claims = verify(&format!(
            "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
              \"exp\":{NOT_EXPIRED},{decoy}}}"
        ))
        .expect("a valid token");
        assert!(
            claims.scope_claim.is_none(),
            "{decoy} must not supply capabilities"
        );
    }
}

/// A tenant claim that is not a version-7 identifier reads as absent.
///
/// Not fatal: the daemon refuses a principal with no tenant anyway, and failing
/// the whole verification would report a provisioning problem as a bad token.
#[test]
fn test_an_unparseable_identifier_claim_reads_as_absent() {
    let claims = verify(&format!(
        "{{\"sub\":\"user_x\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
          \"exp\":{NOT_EXPIRED},\"tenant_id\":\"tenant_a\",\"workspace_id\":42}}"
    ))
    .expect("the token itself is fine");
    assert!(claims.tenant.is_none());
    assert!(claims.workspace_scope.is_none());
}

/// The signing key is the Zig tree's, byte for byte.
#[test]
fn test_the_signing_key_is_the_zig_daemons_fixture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate sits three levels under the repository root")
        .join("src/agentsfleetd/auth/crypto/rs256_sign.zig");
    let zig = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert!(
        zig.contains(&format!("\"{TEST_KEY_PKCS1_B64}\"")),
        "the signing fixture does not match rs256_sign.zig"
    );
}
