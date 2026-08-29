//! Signing real tokens with the Zig tree's throwaway key.
//!
//! Shared because reading claims off an UNSIGNED token would prove nothing —
//! this verifier refuses to read a claim for a decision before the signature
//! verifies, so any test about claim shape has to get past the signature first.
//! One fixture, so two test targets cannot drift in what they consider a
//! well-formed token.
//!
//! The key is `auth/crypto/rs256_sign.zig`'s `TEST_KEY_PKCS1_B64` — generated
//! offline, never used in production, and already embedded there to drive the
//! signer. `claim_shapes` pins that byte-for-byte.
//!
//! The allow sits HERE rather than on each suite that includes it. Every test
//! file carries its own copy for its own body, but `identity_suite` declares
//! `support` once at the crate root, so a suite's inner attribute never reaches
//! this module — the lint would fire on shared scaffolding no test file owns.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_auth::credential::Presented;
use afd_auth::verifier::{TokenVerifier, VerifiedClaims, VerifyError};
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::{JwksVerifier, StaticKeySet, VerifierConfig};
use aws_lc_rs::signature::KeyPair as _;
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

/// The instant every verification below is judged at, in milliseconds.
///
/// Named because two test targets assert time bounds against it: `exp` and
/// `nbf` are only meaningful relative to a clock a test can state.
pub(crate) const NOW_MS: i64 = 1_704_067_400_000;
/// The same instant in seconds, which is the unit both time claims carry.
pub(crate) const NOW_S: i64 = NOW_MS / 1_000;

/// A throwaway 2048-bit RSA key as PKCS#1 DER, base64 without PEM armor.
///
/// `auth/crypto/rs256_sign.zig`'s `TEST_KEY_PKCS1_B64`, pinned to it below.
pub(crate) const TEST_KEY_PKCS1_B64: &str = "MIIEowIBAAKCAQEAxBs4iJWrDhpuy4GQyfrQhtnrXhzEM86cswmwrs9ouW5S4cCi+yzb+xsMZrK2n1AkVkep6c56My6P/13awSMYdtejrSs/b71W+iE83XSWPJJI4sjzUJ0UEU/AQMiMW6LVmWU55n25NyhVOrLxqO3DI5Kb6qlCxDL1yXgyKEmls1e0qXQD2kigsJp6QhcxXgPhAX6wUL0nhSUACPFG468iRU3DLR66dAsTcy7FjNWxh8ljC8ScM9Rm6yNo9i9CGTQQIRwAolMpIMcSxpBKEIhZpwkiEgtwkSvI1s+u5GxSZ6IyBM9tooyb1TlRsWhYm9pkrroGeG0Y3YSdZawXOWrEUwIDAQABAoIBAFC5J8dJXJU8mjjZB6GsxeOMlo8x5i2xMd2c8oayx9f0qtdUtYIREChIFQ29KOFhWuPNMgsVPEYPN6UVnDN+X9ajozNoJv+2/7OMtQIvuJwMV0ZLE6UuU5Fgs7G3G9eoqqYu/et7+x7SUmsMN9+ip33gHqA0tlAO7g/Vk0f0MOomSYGg85ClU9tUVqWS9WOZk7dDcF0zmXDG2aoZEE0oSV62ysQqtkX6ClC2XX4ZtiaBrPGEMB5yxNr6uPiHj7p0IAJtpxRa7jJ5ylWMYqqYGVsGBRkxYsFIDfXs79oxrs9Jf93wZ7A/yyhgWgU9B05LiO8jZ29VlMyu2BqgvP6ITzECgYEA+0UWu8O9vKEMOq2w8AZh9rDQL/L/mJNVFwKok6j6uBQgdvgN5M/ga8tfXj+PR8slFBhydDj80lESxNWwgTzcn7bNglSxdV4A+gCa01o5W6XE0mSe/hug+7pIR2wO9UYNT0gh10Av0xyUn62dLq2qBT60D0HzX57x5Axv6+Ua3TkCgYEAx8xKLWD18oavkQKucVXR/vTb8OWX6qrKG6IFEtzxOAyaRXN7y/cB7rJdl91ytTvZ4djc3lz+Zj9n3DU3HTtj85MktyomawKNpSif4BMx1MzS7cMX24y8ixBzHhroCObu4h200AIWEs3/4HhafTBVLj8tY65WiPfvqrYQPuKAOesCgYEAh3K2zoC1xvkJnpgCyWCnblPh5fcX0Seatsy4EuEERjaTSY5t7uogD/uRbTzV/92CH1MOX5hYsQcDFxgaDZDBXVctcRQ2lQ4XeKzayRPZ142Ei+Wxz0kVfpzsWZPmfFFG23YGyAHRxfuiInF0SbVT8X/bkF38047a1hPeQUs/MAECgYBQLQebPCyWHTw4ycWsz06MrD/SZJ/Y2J5wBk1Y63aVEmGZ+ySzjbSlz8fFGGVemtztR3Qie1jPOSR5dpVeUqXiaaqzIeP2zzh+DVZSugEmLud55+8b+Fb0yy4W558za1BzRo53Zk7rTuUec82ELTARdeLF/IDXR/9SFutgAM6J7wKBgF/WfKsWeV++aRYXS7vsJqq9xM+P1y9JNcIUtItVA7eYe9vbm8/mQ5e1Qln45k1EgzzkcYBBVbuTF5d92xMAHLfdZUjRCDMc752b9B6i1pgPUnd8w1YDoYK7V/wVavOhXuNPc+btdItLFps0+eOa2NCmJ7G4ekqIAvrTRwmwKlJa";

pub(crate) const KID: &str = "signing-fixture";
pub(crate) const ISSUER: &str = "https://clerk.dev.agentsfleet.net";
pub(crate) const AUDIENCE: &str = "https://api.agentsfleet.net";
/// A valid version-7 identifier — the shape a tenant claim must have.
pub(crate) const TENANT: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
pub(crate) const WORKSPACE: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
/// Far in the future, so nothing here is about expiry.
pub(crate) const NOT_EXPIRED: i64 = 4_102_444_800;

pub(crate) fn key_pair() -> aws_lc_rs::signature::RsaKeyPair {
    // `public_key` lives on the `KeyPair` trait in aws-lc-rs, where ring had it
    // as an inherent method — the one place "ring-compatible" is near-compatible.
    let der = STANDARD
        .decode(TEST_KEY_PKCS1_B64)
        .expect("the fixture key is base64");
    aws_lc_rs::signature::RsaKeyPair::from_der(&der).expect("a PKCS#1 RSA private key")
}

/// A key set publishing the signing key's own public components.
pub(crate) fn key_set() -> String {
    let pair = key_pair();
    let components: aws_lc_rs::signature::RsaPublicKeyComponents<Vec<u8>> =
        pair.public_key().into();
    let n = URL_SAFE_NO_PAD.encode(&components.n);
    let e = URL_SAFE_NO_PAD.encode(&components.e);
    format!("{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{KID}\",\"n\":\"{n}\",\"e\":\"{e}\"}}]}}")
}

/// Signs `payload` into a compact RS256 token.
pub(crate) fn sign(payload: &str) -> Presented {
    let header = URL_SAFE_NO_PAD.encode(format!(
        "{{\"alg\":\"RS256\",\"typ\":\"JWT\",\"kid\":\"{KID}\"}}"
    ));
    let body = URL_SAFE_NO_PAD.encode(payload);
    let signing_input = format!("{header}.{body}");

    let pair = key_pair();
    let mut signature = vec![0_u8; pair.public_key().modulus_len()];
    pair.sign(
        &aws_lc_rs::signature::RSA_PKCS1_SHA256,
        &aws_lc_rs::rand::SystemRandom::new(),
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

pub(crate) fn verify(payload: &str) -> Result<VerifiedClaims, VerifyError> {
    verify_at(payload, NOW_MS)
}

/// The same, judged at a caller-chosen instant.
///
/// Separate entry point rather than a defaulted argument so the tests that do
/// not care about time keep reading as tests about claim shape.
pub(crate) fn verify_at(payload: &str, now_ms: i64) -> Result<VerifiedClaims, VerifyError> {
    let verifier = JwksVerifier::new(
        StaticKeySet::new(key_set().into_bytes()),
        VerifierConfig::new(ISSUER, AUDIENCE),
        Arc::new(FixedClock::at(UnixMillis::from_millis(now_ms))) as Arc<dyn Clock>,
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(verifier.verify(&sign(payload)))
}
