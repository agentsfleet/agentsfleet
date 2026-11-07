//! Published RSA signing keys, decoded by jsonwebtoken and restricted to RS256.

use crate::error::Result;
use afd_auth::verifier::VerifyError;
use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, KeyAlgorithm, KeyOperations, PublicKeyUse};

/// Smallest modulus this daemon will verify against, in bytes (2048 bits).
///
/// Equal to `RSA_PKCS1_2048_8192_SHA256`'s own floor: naming it here rather
/// than trusting the constant means a key that would be rejected deep inside
/// ring is rejected at parse, where the reason can be reported.
pub const MIN_MODULUS_BYTES: usize = 256;
/// Largest, also matching the verification parameters (8192 bits).
pub const MAX_MODULUS_BYTES: usize = 1024;

/// A published signing key with its prepared verification material.
#[derive(Debug, Clone)]
pub struct SigningKey {
    kid: Box<str>,
    pub(crate) decoding_key: jsonwebtoken::DecodingKey,
}

impl SigningKey {
    /// The identifier a token's `kid` header selects this key by.
    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }
}

/// A key set as published, parsed down to what this daemon can use.
#[derive(Debug, Clone, Default)]
pub struct JwkKeySet {
    keys: Vec<SigningKey>,
    rejected: usize,
}

impl JwkKeySet {
    /// The key matching `kid`, or `None` when the set does not carry it.
    ///
    /// A miss is what triggers a refresh — an issuer that rotated keys
    /// publishes the new one before it signs with it, so a `kid` this set does
    /// not know usually means the set is simply old.
    #[must_use]
    pub fn find(&self, kid: &str) -> Option<&SigningKey> {
        self.keys.iter().find(|key| &*key.kid == kid)
    }

    /// How many usable keys the set carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the set carries no usable key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// How many published keys this daemon declined — wrong key type, missing
    /// components, or a modulus outside the verifiable range.
    ///
    /// Reported rather than discarded so boot can say "the issuer published
    /// three keys and this daemon can use none of them", which is a different
    /// operator problem from "the issuer published nothing".
    #[must_use]
    pub fn rejected(&self) -> usize {
        self.rejected
    }

    /// Whether this set can verify anything at all.
    ///
    /// The check §7 runs at boot. A set with keys the daemon cannot use is a
    /// configuration failure that must refuse startup, not a per-request 401
    /// that looks like every user's token going bad at once.
    #[must_use]
    pub fn usable(&self) -> bool {
        !self.is_empty()
    }

    /// Parses a published JWKS document.
    ///
    /// # Errors
    /// [`VerifyError::KeySetUnavailable`] when the document does not parse, or
    /// carries no key this daemon can verify against. Both are the issuer's
    /// problem rather than the caller's, so neither is a rejection.
    ///
    /// Individual unusable keys are SKIPPED rather than fatal: a set may
    /// legitimately publish an EC key beside an RSA one, and refusing the whole
    /// document over a key we were never going to use would be an outage
    /// authored by strictness.
    pub fn parse(raw: &[u8]) -> Result<Self, VerifyError> {
        let doc: Document<'_> = afd_core::json::object_from_slice(raw)
            .map_err(|_invalid| VerifyError::KeySetUnavailable)?;
        let mut keys = Vec::with_capacity(doc.keys.len());
        let mut rejected = 0_usize;
        for jwk in doc.keys {
            match signing_key(jwk) {
                Some(key) => {
                    if keys.iter().any(|held: &SigningKey| held.kid == key.kid) {
                        return Err(VerifyError::KeySetUnavailable);
                    }
                    keys.push(key);
                }
                None => rejected = rejected.saturating_add(1),
            }
        }
        if keys.is_empty() {
            return Err(VerifyError::KeySetUnavailable);
        }
        Ok(Self { keys, rejected })
    }
}

/// The published document's shape. Unknown fields are ignored — a key set
/// carries provider-specific extras and refusing them would be brittle.
#[derive(Debug, serde::Deserialize)]
struct Document<'a> {
    #[serde(borrow)]
    keys: Vec<&'a serde_json::value::RawValue>,
}

/// Malformed or incompatible individual keys do not discard usable siblings.
fn signing_key(value: &serde_json::value::RawValue) -> Option<SigningKey> {
    let jwk: Jwk = serde_json::from_str(value.get()).ok()?;
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| *usage != PublicKeyUse::Signature)
        || jwk
            .common
            .key_algorithm
            .is_some_and(|alg| alg != KeyAlgorithm::RS256)
        || jwk
            .common
            .key_operations
            .as_ref()
            .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
    {
        return None;
    }
    let AlgorithmParameters::RSA(ref rsa) = jwk.algorithm else {
        return None;
    };
    let modulus = decode_component(&rsa.n)?;
    if !(MIN_MODULUS_BYTES..=MAX_MODULUS_BYTES).contains(&modulus.len()) {
        return None;
    }
    let decoding_key = jsonwebtoken::DecodingKey::from_jwk(&jwk).ok()?;
    let kid = jwk.common.key_id.filter(|kid| !kid.is_empty())?;
    Some(SigningKey {
        kid: kid.into(),
        decoding_key,
    })
}

/// Decodes the modulus to enforce the daemon's supported RSA size range.
fn decode_component(value: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
}
