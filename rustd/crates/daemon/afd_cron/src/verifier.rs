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
//! between the vendor's rotation and the update of this daemon's `qstash` bag.

use af_crypto::secret::SecretString;
use garde::Unvalidated;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

use crate::error::{self, Rejected, Result};

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

/// The fire token as it arrived, with the bound and the shape it must hold
/// before the decoder reads a byte of it.
///
/// Declared on the field rather than checked by hand: an empty header, an
/// oversized one and one that is not three base64url segments joined by dots
/// are the same refusal. The shape is the compact form a signed token takes, so
/// a request that is not even that costs the keys' datastore read nothing.
#[derive(Debug, Clone, Copy, garde::Validate)]
#[garde(transparent)]
struct Token<'a>(
    #[garde(
        length(bytes, min = 1, max = MAX_TOKEN_BYTES),
        pattern(r"^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$")
    )]
    &'a str,
);

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

/// Refuses a token past the bound, or not shaped like one, before anything is
/// read to check it.
///
/// Run ahead of the credentials read, so a request with no token, an oversized
/// one or a string that is no token at all, the cheapest things a stranger can
/// send, costs no datastore round trip.
///
/// # Errors
/// The `token_malformed` refusal, for an empty token, one past
/// [`MAX_TOKEN_BYTES`], or one that is not three base64url segments joined by
/// dots.
pub(crate) fn bounded(token: &str) -> Result<()> {
    Unvalidated::new(Token(token))
        .validate()
        .map(|_within| ())
        .map_err(|_bound| error::token_malformed())
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
    // Checked only when present unless required: a signed token with no issuer or
    // no subject would otherwise skip both checks. A fire carries all three.
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    validation.sub = Some(destination.to_owned());
    // The crate's 10.x default REJECTS a token carrying any `aud` — see the
    // module note. This surface has no audience to check, so the check is
    // turned off by name rather than left to a default that changed once.
    validation.validate_aud = false;
    validation.validate_exp = true;
    // QStash never sets `nbf`: its Go server writes the zero time, which is
    // -62135596800 seconds, a value no unsigned claim parser accepts. The token
    // is still bound by its signature, `exp`, `sub` and the digest of the body,
    // so a start-of-validity check adds nothing the others do not already give.
    validation.validate_nbf = false;
    // No leeway. The scheduler and this daemon both run on synchronised clocks,
    // and a tolerance here is a window a replayed token lives inside.
    validation.leeway = 0;

    let mut cause = Rejected::Signature;
    let mut decoded = None;
    for key in [keys.current.expose(), keys.next.expose()]
        .into_iter()
        .filter(|key| !key.is_empty())
    {
        let attempt = jsonwebtoken::decode::<Claims>(
            token.0,
            &DecodingKey::from_secret(key.as_bytes()),
            &validation,
        );
        match attempt {
            Ok(found) => {
                decoded = Some(found);
                break;
            }
            // A wrong key fails the signature before any claim is read, so the
            // first failure that is anything else is what the right key tripped
            // on. Kept for the operator's log; the sender gets one answer.
            Err(failure) if cause == Rejected::Signature => cause = rejected(failure.kind()),
            Err(_failure) => {}
        }
    }

    let Some(decoded) = decoded else {
        return Err(error::signature_invalid(cause));
    };

    // `jti` is the at-most-once claim key: an empty one would let every fire share
    // a single key, so it counts as absent.
    if decoded.claims.jti.is_empty() {
        return Err(error::signature_invalid(Rejected::Claims));
    }

    if !body_matches(&decoded.claims.body, body) {
        return Err(error::body_mismatch());
    }

    Ok(VerifiedFire {
        message_id: decoded.claims.jti,
    })
}

/// Whether the `body` claim is the digest of the bytes that arrived.
///
/// Base64url, read without its padding: the scheduler pads the digest it signs and
/// this daemon computes it unpadded. Compared as text
/// rather than decoded first: both sides are a fixed-length digest of the same
/// alphabet, so a decode would add a failure mode without adding a check.
///
/// Compared through [`subtle`], the same primitive `af_crypto::mac` compares a
/// tag with (RULE CTM). `ct_eq` short-circuits on LENGTH and not on content,
/// which is the property that matters: the length of a base64 digest is public,
/// and what must not leak is how many characters of it matched.
fn body_matches(claimed: &str, body: &[u8]) -> bool {
    let expected = base64_url_no_pad(&Sha256::digest(body));
    // The scheduler pads the digest it signs, and its own SDKs strip the padding
    // before they compare. Accept exactly the digest, or the digest with the one
    // `=` a 32-byte hash pads to; any other tail is not the scheduler's.
    let padded = format!("{expected}=");
    let same =
        expected.as_bytes().ct_eq(claimed.as_bytes()) | padded.as_bytes().ct_eq(claimed.as_bytes());
    same.unwrap_u8() == 1
}

/// The base64url, unpadded rendering the scheduler uses.
fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The check a failed decode names, as the operator's log words it.
fn rejected(kind: &jsonwebtoken::errors::ErrorKind) -> Rejected {
    use jsonwebtoken::errors::ErrorKind;
    match kind {
        ErrorKind::ExpiredSignature => Rejected::Expired,
        ErrorKind::InvalidIssuer => Rejected::Issuer,
        ErrorKind::InvalidSubject => Rejected::Subject,
        ErrorKind::MissingRequiredClaim(_) => Rejected::Claims,
        ErrorKind::InvalidClaimFormat(_) => Rejected::ClaimFormat,
        ErrorKind::InvalidAlgorithm | ErrorKind::MissingAlgorithm => Rejected::Algorithm,
        ErrorKind::InvalidToken
        | ErrorKind::Base64(_)
        | ErrorKind::Json(_)
        | ErrorKind::Utf8(_) => Rejected::Undecodable,
        _other => Rejected::Signature,
    }
}
