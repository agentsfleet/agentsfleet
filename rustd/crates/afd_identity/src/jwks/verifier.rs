//! Verifying a session token: signature first, then the standard claims.
//!
//! # Order is a security property, not a style
//!
//! The signature is checked BEFORE any claim is read for a decision. A verifier
//! that trusted `iss` to pick a key, or refused on `exp` before checking the
//! signature, would be acting on an attacker-controlled payload. The only
//! pre-signature read is the header's `kid`, which selects a key from a set the
//! ISSUER published — an attacker naming a `kid` we do not hold gets
//! `KeyNotFound`, not a key of their choosing.
//!
//! # Why the claim checks are ours rather than a crate's
//!
//! `jsonwebtoken` would do all of this, and `core_api-develop` uses it — then
//! hand-writes `has_correct_issuer`, `has_correct_subject`,
//! `has_secret_name_claim` and `has_account_uid_claim` on top, because the
//! crate's built-in `Validation` did not fit. The same wall is here: it
//! validates `exp` against `SystemTime` with no seam, and Dimension 4.2 needs
//! an expired-token test that does not depend on the wall clock. Owning the
//! four checks costs forty lines and keeps every one of them steerable.

use std::sync::Arc;

use afd_auth::credential::Presented;
use afd_auth::principal::Subject;
use afd_auth::verifier::{TokenVerifier, VerifiedClaims, VerifyError};
use afd_core::clock::Clock;
use afd_core::id::Uuid7;

use crate::jwks::cache::{DEFAULT_TTL_MS, KeyCache};
use crate::jwks::key_set::SigningKey;
use crate::jwks::source::KeySetSource;
use crate::jwt::{Header, Segments, decode_segment};

/// What a deployment must agree with its identity provider about.
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    /// The required `iss` claim. `OIDC_ISSUER`, and also the base the key-set
    /// URL is derived from, so the two can never name different providers.
    pub issuer: Box<str>,
    /// The required `aud` claim. `OIDC_AUDIENCE`, checked STRICTLY.
    ///
    /// Strictness is what makes a leaked token un-replayable against a sibling
    /// service: each service checks only its own audience, so cross-service
    /// replay is refused by the verifier rather than by application logic
    /// (`docs/AUTH.md` §Per-microservice JWT templates).
    pub audience: Box<str>,
    /// How long a fetched key set is served without asking again.
    pub ttl_ms: i64,
}

impl VerifierConfig {
    /// A configuration with the documented six-hour key-set lifetime.
    #[must_use]
    pub fn new(issuer: impl Into<Box<str>>, audience: impl Into<Box<str>>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            ttl_ms: DEFAULT_TTL_MS,
        }
    }
}

/// Verifies RS256 session tokens against an issuer's published key set.
#[derive(Debug)]
pub struct JwksVerifier<S> {
    cache: KeyCache<S>,
    clock: Arc<dyn Clock>,
    issuer: Box<str>,
    audience: Box<str>,
}

impl<S: KeySetSource> JwksVerifier<S> {
    /// Builds a verifier over `source`.
    #[must_use]
    pub fn new(source: S, config: VerifierConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            cache: KeyCache::new(source, Arc::clone(&clock), config.ttl_ms),
            clock,
            issuer: config.issuer,
            audience: config.audience,
        }
    }

    /// Fetches the key set once, so boot can refuse a provider it cannot use.
    ///
    /// The Zig daemon's `checkJwksConnectivity`, which is wired only to
    /// `cmd/doctor.zig:283` and not to serve. §7 calls this at boot instead:
    /// a key set with no key this daemon can verify against would otherwise
    /// 401 every session token while `agt_t` and `afc_` kept working — the
    /// "signed in, but nothing loads" signature `docs/AUTH.md` records.
    ///
    /// # Errors
    /// [`VerifyError::KeySetUnavailable`] when the provider is unreachable, the
    /// document does not parse, or it carries no usable key.
    /// A set with no usable key never reaches here: `JwkKeySet::parse` refuses
    /// the whole document in that case, so priming already failed. The check
    /// that would restate it is left out rather than written as a branch no
    /// input can take.
    pub async fn prime(&self) -> Result<(), VerifyError> {
        self.cache.prime().await?;
        Ok(())
    }

    /// The key-set source, for a test to count fetches against.
    #[must_use]
    pub const fn source(&self) -> &S {
        self.cache.source()
    }

    /// The whole verification, in the order that makes the order matter.
    async fn verify_token(&self, token: &str) -> Result<VerifiedClaims, VerifyError> {
        let segments = Segments::split(token)?;
        let header_raw = decode_segment(segments.header)?;
        let (_header, kid) = Header::parse(&header_raw)?;

        let keys = self.cache.resolve(&kid).await?;
        let key = keys.find(&kid).ok_or(VerifyError::KeyNotFound)?;

        let signature = decode_segment(segments.signature)?;
        verify_rs256(key, segments.signing_input().as_bytes(), &signature)?;

        // Only now is the payload something a decision may be based on.
        let payload_raw = decode_segment(segments.payload)?;
        self.read_claims(&payload_raw)
    }

    /// Checks the standard claims and lifts the ones this daemon acts on.
    fn read_claims(&self, payload: &[u8]) -> Result<VerifiedClaims, VerifyError> {
        let claims: Claims =
            serde_json::from_slice(payload).map_err(|_invalid| VerifyError::Malformed)?;

        if claims.iss.as_deref() != Some(&*self.issuer) {
            return Err(VerifyError::IssuerMismatch);
        }
        if !claims.audience_contains(&self.audience) {
            return Err(VerifyError::AudienceMismatch);
        }
        let exp = claims.exp.ok_or(VerifyError::MissingClaim)?;
        // `exp` is seconds since the epoch; the clock reads milliseconds.
        // Comparing in seconds is what the Zig daemon does
        // (`jwks_standard_claims.zig`: `if (exp <= now_s)`), including the
        // boundary: a token expiring exactly now is expired.
        if exp <= self.clock.now().as_seconds() {
            return Err(VerifyError::Expired);
        }
        let subject = claims
            .sub
            .as_deref()
            .ok_or(VerifyError::MissingClaim)
            .and_then(|sub| Subject::new(sub).map_err(|_blank| VerifyError::MissingClaim))?;

        Ok(VerifiedClaims {
            subject,
            // A claim that is present but unparseable is treated as absent
            // rather than fatal: the daemon refuses a principal with no tenant
            // anyway, and failing the whole verification would report a
            // provisioning problem as a bad token.
            tenant: claims.identifier(CLAIM_TENANT_ID),
            workspace_scope: claims.identifier(CLAIM_WORKSPACE_ID),
            scope_claim: claims.scopes.map(Into::into),
        })
    }
}

impl<S: KeySetSource> TokenVerifier for JwksVerifier<S> {
    fn verify(
        &self,
        presented: &Presented,
    ) -> impl Future<Output = Result<VerifiedClaims, VerifyError>> + Send {
        // The credential is copied out here rather than borrowed into the
        // future: `Presented` zeroes on drop, and holding a borrow across an
        // await would tie the caller's lifetime to this verification.
        let token = presented.expose().to_owned();
        async move { self.verify_token(&token).await }
    }
}

/// The tenant claim's name, in both places it may appear.
const CLAIM_TENANT_ID: &str = "tenant_id";
/// The workspace-ceiling claim's name.
const CLAIM_WORKSPACE_ID: &str = "workspace_id";
/// The object the provider nests its metadata claims under.
///
/// `clerk_metadata_payload.zig` writes exactly two keys into `public_metadata`,
/// and the session-token template projects `metadata.tenant_id` — so on a real
/// deployment the tenant is NESTED, and a reader that only looked at the top
/// level would find it on no production token at all.
const CLAIM_METADATA: &str = "metadata";

/// The claims this daemon reads. Everything else the issuer sends is ignored.
#[derive(Debug, serde::Deserialize)]
struct Claims {
    sub: Option<String>,
    iss: Option<String>,
    aud: Option<serde_json::Value>,
    exp: Option<i64>,
    /// Everything else, so the nested metadata object stays reachable without
    /// naming a second struct for one lookup.
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
    /// The capability claim, top level ONLY.
    ///
    /// `claims.zig` is emphatic about this: an earlier ladder tried `OAuth2`'s
    /// `scope` BEFORE this one, so a token carrying a standard `scope` claim
    /// would silently have supplied a different capability set. One place, and
    /// a reader that cannot say which value it trusted is the bug.
    scopes: Option<String>,
}

impl Claims {
    /// An identifier claim, read top-level first and then under `metadata`.
    ///
    /// The ladder `claims.zig::getClerkTenantId` walks, and in that order: a
    /// top-level projection wins over the nested one, so a template that starts
    /// projecting to the top level does not need both readers changed at once.
    fn identifier(&self, name: &str) -> Option<Uuid7> {
        let raw = self
            .rest
            .get(name)
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                self.rest
                    .get(CLAIM_METADATA)?
                    .as_object()?
                    .get(name)?
                    .as_str()
            })?;
        Uuid7::parse(raw).ok()
    }

    /// Whether `aud` names `wanted`, as a string or inside an array.
    ///
    /// Both shapes are legal in the specification and providers use both, so
    /// accepting only one would refuse a conforming token.
    fn audience_contains(&self, wanted: &str) -> bool {
        match &self.aud {
            Some(serde_json::Value::String(one)) => one == wanted,
            Some(serde_json::Value::Array(many)) => many
                .iter()
                .any(|item| item.as_str().is_some_and(|value| value == wanted)),
            _ => false,
        }
    }
}

/// Checks an RS256 signature over `message`.
///
/// # Errors
/// [`VerifyError::SignatureInvalid`] for every failure. ring reports one
/// opaque error by design, and that is the right shape here too: a caller
/// learning WHY a signature failed learns something about the key or the
/// padding, and neither is theirs to know.
fn verify_rs256(key: &SigningKey, message: &[u8], signature: &[u8]) -> Result<(), VerifyError> {
    aws_lc_rs::signature::RsaPublicKeyComponents {
        n: key.modulus(),
        e: key.exponent(),
    }
    .verify(
        &aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256,
        message,
        signature,
    )
    .map_err(|_unspecified| VerifyError::SignatureInvalid)
}
