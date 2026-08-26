//! The signing keys an issuer publishes, and what this daemon accepts of them.
//!
//! # The key-size floor is a deliberate divergence
//!
//! `src/agentsfleetd/auth/jwks_crypto.zig` accepts moduli of 128, 256, 384 and
//! 512 bytes — 1024 bits upward. This daemon accepts 2048 upward, because
//! `aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256` is what it verifies with, and
//! ring names its 1024-bit constant `FOR_LEGACY_USE_ONLY` on purpose.
//!
//! No production identity provider publishes 1024-bit RSA, so the divergence is
//! unreachable in practice. It is still a divergence, and parity is one-way, so
//! it is recorded in the milestone spec rather than left to be discovered.
//!
//! What matters more is that the divergence cannot fail SILENTLY. A key set
//! this daemon will not verify against would otherwise 401 every session token
//! while `agt_t` and `afc_` kept working — "signed in, but nothing loads", the
//! signature `docs/AUTH.md` already records for the gzip bug. So a key that
//! fails the floor is dropped at PARSE time with a reason, and
//! [`JwkKeySet::usable`] lets boot refuse rather than serve.

use afd_auth::verifier::VerifyError;

/// Smallest modulus this daemon will verify against, in bytes (2048 bits).
///
/// Equal to `RSA_PKCS1_2048_8192_SHA256`'s own floor: naming it here rather
/// than trusting the constant means a key that would be rejected deep inside
/// ring is rejected at parse, where the reason can be reported.
pub const MIN_MODULUS_BYTES: usize = 256;
/// Largest, also matching the verification parameters (8192 bits).
pub const MAX_MODULUS_BYTES: usize = 1024;

/// One RSA public key from a key set, decoded to the components ring takes.
///
/// `modulus` and `exponent` are big-endian bytes with no leading zeros, which
/// is both what base64url-decoding a JWK's `n`/`e` yields and exactly what
/// `aws_lc_rs::signature::RsaPublicKeyComponents` expects — the reason this daemon
/// verifies with ring rather than through rustls. rustls does verify RSA, but
/// only against a DER `SubjectPublicKeyInfo`, because in TLS a key always
/// arrives inside a certificate; reaching it would mean hand-encoding ASN.1
/// around components ring already accepts directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningKey {
    kid: Box<str>,
    modulus: Box<[u8]>,
    exponent: Box<[u8]>,
}

impl SigningKey {
    /// The identifier a token's `kid` header selects this key by.
    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// The modulus, big-endian.
    #[must_use]
    pub fn modulus(&self) -> &[u8] {
        &self.modulus
    }

    /// The public exponent, big-endian.
    #[must_use]
    pub fn exponent(&self) -> &[u8] {
        &self.exponent
    }
}

/// A key set as published, parsed down to what this daemon can use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        let doc: Document = afd_core::json::object_from_slice(raw)
            .map_err(|_invalid| VerifyError::KeySetUnavailable)?;
        let mut keys = Vec::with_capacity(doc.keys.len());
        let mut rejected = 0_usize;
        for jwk in doc.keys {
            match jwk.into_signing_key() {
                Some(key) => keys.push(key),
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
struct Document {
    keys: Vec<Jwk>,
}

/// One published key, before this daemon decides whether it can use it.
#[derive(Debug, serde::Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

impl Jwk {
    /// Converts to a usable key, or `None` with the reason discarded.
    ///
    /// The reason is discarded on purpose: every one of them means the same
    /// thing to the only consumer — this key cannot verify a token — and
    /// carrying four variants of "unusable" to a caller that treats them
    /// identically would be shape without meaning.
    fn into_signing_key(self) -> Option<SigningKey> {
        // `kty` absent is tolerated for the reason `jwks.zig` tolerates it: it
        // is optional in the JWK spec and a set that omits it is still usable.
        // Present and not RSA is a key for a different algorithm entirely.
        if self.kty.is_some_and(|kty| kty != "RSA") {
            return None;
        }
        let kid = self.kid?;
        let modulus = decode_component(&self.n?)?;
        let exponent = decode_component(&self.e?)?;
        if !(MIN_MODULUS_BYTES..=MAX_MODULUS_BYTES).contains(&modulus.len()) {
            return None;
        }
        Some(SigningKey {
            kid: kid.into(),
            modulus: modulus.into(),
            exponent: exponent.into(),
        })
    }
}

/// Decodes a base64url JWK component to big-endian bytes.
fn decode_component(value: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
}
