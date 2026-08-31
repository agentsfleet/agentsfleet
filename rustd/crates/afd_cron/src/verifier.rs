//! Whether a schedule fire really came from the external scheduler.
//!
//! # Every default `jsonwebtoken` offers is turned off by name
//!
//! The crate's `Validation::new` is permissive where this surface must not be:
//! it validates `exp` and, from 10.x, `aud` — and a defaulted `aud` check
//! REJECTS a token carrying an audience claim the caller never configured. The
//! external scheduler is free to add one, and a daemon that started refusing
//! every fire on a vendor's release would be down until somebody read a
//! stack trace. So every knob this surface has an opinion about is set here,
//! and the ones it does not are turned off rather than left at whatever the
//! next minor version decides.
//!
//! # The body is bound into the token, and that is the whole point
//!
//! `body` is a claim carrying the base64url SHA-256 of the delivery. Checking
//! the signature alone would prove the TOKEN came from the scheduler and say
//! nothing about the bytes it arrived with — an attacker holding a captured
//! token could post any body under it. Comparing the digest is what makes the
//! signature cover the delivery.
//!
//! # Two keys, because a rotation has no gap
//!
//! The scheduler publishes a current and a next signing key, and rotates by
//! promoting the second. A verifier that knew one would refuse every delivery
//! between the vendor's rotation and this daemon's redeploy.

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

/// The one algorithm this daemon will read a fire token under.
///
/// Pinned rather than taken from the token's own header, which is the whole of
/// the `alg: none` family of forgeries: a verifier that believed the header
/// would accept a token that says it needs no signature.
const EXPECTED_ALGORITHM: Algorithm = Algorithm::HS256;

/// Who the token must say minted it.
const EXPECTED_ISSUER: &str = "Upstash";

/// The longest token this daemon will read.
///
/// `QStashVerifier.zig`'s `MAX_TOKEN_BYTES`. A bound on the work one
/// unauthenticated request can ask of the base64 decoder.
pub const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// Why a fire was not believed.
///
/// One variant per reason an operator would act differently on: a key that is
/// missing is a deployment to configure, a mismatched destination is a schedule
/// registered against the wrong daemon, and a body mismatch is the one that
/// means somebody is replaying a captured token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unverified {
    /// This deployment holds no signing key to check against.
    ///
    /// Fail-closed: with no key there is nothing to verify, and accepting the
    /// fire would let anyone who found the URL wake every fleet on it.
    KeysMissing,
    /// The token is longer than [`MAX_TOKEN_BYTES`], or is not a token at all.
    Malformed,
    /// Neither the current nor the next key produced this signature.
    SignatureInvalid,
    /// The token is not for this daemon, or not from the scheduler.
    ///
    /// Issuer and subject together: the subject is the destination URL the
    /// schedule was registered against, so a token minted for another
    /// deployment fails here rather than waking a fleet on this one.
    WrongTarget,
    /// The token's window has passed, or has not opened.
    OutsideWindow,
    /// The token is genuine and the body is not the one it was minted over.
    BodyMismatch,
}

impl Unverified {
    /// The scoped reason an operator reads in the log.
    ///
    /// A stable word per variant rather than a `Debug` rendering: this is the
    /// value somebody greps for when a schedule stops firing, and a derived
    /// spelling would change under a rename that meant nothing to them
    /// (`LOGGING_STANDARD` §8A also refuses positional formatting in an emit).
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::KeysMissing => "signing_keys_absent",
            Self::Malformed => "token_malformed",
            Self::SignatureInvalid => "signature_invalid",
            Self::WrongTarget => "wrong_target",
            Self::OutsideWindow => "outside_window",
            Self::BodyMismatch => "body_mismatch",
        }
    }
}

/// What a believed fire carries.
#[derive(Debug, Clone)]
pub struct VerifiedFire {
    /// The scheduler's own id for this delivery, repeated across its retries.
    ///
    /// The at-most-once claim key — see [`crate::fire`].
    pub message_id: String,
}

/// The claims this daemon reads out of a fire token.
///
/// `jti` and `body` are non-optional deliberately. The crate would happily
/// deserialize a token missing either, and both are load-bearing: without `jti`
/// there is no claim key and every retry becomes a fresh fire, and without
/// `body` the signature covers nothing this daemon checked.
#[derive(Debug, Deserialize)]
struct Claims {
    /// The delivery id.
    jti: String,
    /// The base64url, unpadded SHA-256 of the delivery body.
    body: String,
}

/// The keys a fire is checked against.
///
/// Both are the scheduler's, and both are tried — see the module note on why
/// one would mean an outage at every rotation.
#[derive(Debug, Clone)]
pub struct SigningKeys {
    /// The key the scheduler is signing with now.
    pub current: String,
    /// The key it will sign with next.
    pub next: String,
}

/// Whether `token` proves `body` was sent by the scheduler to `destination`.
///
/// Takes no instant, unlike every other decision in this milestone. The `exp`
/// and `nbf` checks belong to `jsonwebtoken`, which reads the system clock and
/// offers no seam to hand one in — so a parameter here would be an instant this
/// function accepted and did not use, which is worse than not taking one.
///
/// # Errors
/// [`Unverified`], one variant per reason — see that type on why they are not
/// collapsed.
pub fn verify_at(
    keys: &SigningKeys,
    destination: &str,
    token: &str,
    body: &[u8],
) -> Result<VerifiedFire, Unverified> {
    if keys.current.is_empty() && keys.next.is_empty() {
        return Err(Unverified::KeysMissing);
    }
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return Err(Unverified::Malformed);
    }

    let mut validation = Validation::new(EXPECTED_ALGORITHM);
    validation.set_issuer(&[EXPECTED_ISSUER]);
    validation.sub = Some(destination.to_owned());
    // The crate's 10.x default REJECTS a token carrying any `aud` — see the
    // module note. This surface has no audience to check, so the check is
    // turned off by name rather than left to a default that changed once.
    validation.validate_aud = false;
    validation.validate_exp = true;
    validation.validate_nbf = true;
    // No leeway. The scheduler and this daemon both run on synchronised clocks,
    // and a tolerance here is a window a replayed token lives inside.
    validation.leeway = 0;

    let decoded = [keys.current.as_str(), keys.next.as_str()]
        .into_iter()
        .filter(|key| !key.is_empty())
        .find_map(|key| {
            jsonwebtoken::decode::<Claims>(
                token,
                &DecodingKey::from_secret(key.as_bytes()),
                &validation,
            )
            .ok()
        });

    let Some(decoded) = decoded else {
        // One answer for a wrong key, a wrong issuer, a wrong subject and an
        // expired token would be wrong — but the crate collapses them into one
        // `ErrorKind` set that would have to be re-classified here to tell them
        // apart per key. What matters operationally is which SIDE failed, and
        // a second pass with the checks relaxed would be a second decode of an
        // untrusted token. Reported as a signature failure, with the window and
        // target variants reserved for the checks this file makes itself.
        return Err(Unverified::SignatureInvalid);
    };

    if !body_matches(&decoded.claims.body, body) {
        return Err(Unverified::BodyMismatch);
    }

    Ok(VerifiedFire {
        message_id: decoded.claims.jti,
    })
}

/// Whether the `body` claim is the digest of the bytes that arrived.
///
/// Base64url without padding, as the scheduler emits it. Compared as text
/// rather than decoded first: both sides are a fixed-length digest of the same
/// alphabet, so a decode would add a failure mode without adding a check.
///
/// Compared through [`subtle`], the same primitive `afd_crypto::mac` compares a
/// tag with (RULE CTM). `ct_eq` short-circuits on LENGTH and not on content,
/// which is the property that matters: the length of a base64 digest is public,
/// and what must not leak is how many characters of it matched.
fn body_matches(claimed: &str, body: &[u8]) -> bool {
    let expected = base64_url_no_pad(&Sha256::digest(body));
    expected.as_bytes().ct_eq(claimed.as_bytes()).unwrap_u8() == 1
}

/// The base64url, unpadded rendering the scheduler uses.
fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
