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

use afd_crypto::secret::SecretString;
use garde::Unvalidated;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

use crate::error::{self, Result};

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
/// A bound on the work one unauthenticated request can ask of the base64
/// decoder.
pub const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// The fire token as it arrived, with the bound it must hold before the
/// decoder reads a byte of it.
///
/// Declared on the field rather than checked by hand: an empty header and an
/// oversized one are the same refusal, [`MAX_TOKEN_BYTES`].
#[derive(Debug, Clone, Copy, garde::Validate)]
#[garde(transparent)]
struct Token<'a>(#[garde(length(bytes, min = 1, max = MAX_TOKEN_BYTES))] &'a str);

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
    pub current: SecretString,
    /// The key it will sign with next.
    pub next: SecretString,
}

/// Whether `token` proves `body` was sent by the scheduler to `destination`.
///
/// Takes no instant, unlike every other decision in this milestone. The `exp`
/// and `nbf` checks belong to `jsonwebtoken`, which reads the system clock and
/// offers no seam to hand one in — so a parameter here would be an instant this
/// function accepted and did not use, which is worse than not taking one.
///
/// # Errors
/// One kind per reason a fire is not believed, read through
/// [`crate::Error::refusal_word`]: a missing key is a deployment to configure, a body
/// mismatch is somebody replaying a captured token. The keys are checked
/// before the token, so a deployment that cannot verify says so whatever
/// arrived.
pub fn verify_at(
    keys: &SigningKeys,
    destination: &str,
    token: &str,
    body: &[u8],
) -> Result<VerifiedFire> {
    if keys.current.is_empty() && keys.next.is_empty() {
        return Err(error::keys_missing());
    }
    let token = Unvalidated::new(Token(token))
        .validate()
        .map_err(|_bound| error::token_malformed())?;

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

    let decoded = [keys.current.expose(), keys.next.expose()]
        .into_iter()
        .filter(|key| !key.is_empty())
        .find_map(|key| {
            jsonwebtoken::decode::<Claims>(
                token.0,
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
        // untrusted token. Reported as a signature failure.
        return Err(error::signature_invalid());
    };

    if !body_matches(&decoded.claims.body, body) {
        return Err(error::body_mismatch());
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
