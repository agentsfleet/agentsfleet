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
//! `jsonwebtoken` owns JWS verification. The claim adapter retains the injected
//! clock, zero leeway, exact expiry boundary, and workspace confinement policy.

use std::sync::Arc;

use afd_auth::credential::Presented;
use afd_auth::principal::Subject;
use afd_auth::verifier::{TokenVerifier, VerifiedClaims, VerifyError};
use afd_core::clock::Clock;

use crate::error::Result;
use crate::jwks::cache::{DEFAULT_TTL_MS, KeyCache};
use crate::jwks::claims::{CLAIM_TENANT_ID, Claims};
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

        let claims = verify_claims(key, segments)?;
        self.read_claims(claims)
    }

    /// Checks the standard claims and lifts the ones this daemon acts on.
    fn read_claims(&self, claims: Claims) -> Result<VerifiedClaims, VerifyError> {
        if claims.iss.as_deref() != Some(&*self.issuer) {
            return Err(VerifyError::IssuerMismatch);
        }
        if !claims.audience_contains(&self.audience) {
            return Err(VerifyError::AudienceMismatch);
        }
        // One clock read for both time bounds, so they can never disagree about
        // when "now" was.
        let now = self.clock.now().as_seconds();
        let exp = claims.exp.ok_or(VerifyError::MissingClaim)?;
        // `exp` is seconds since the epoch; the clock reads milliseconds.
        // Comparing in seconds is what the Zig daemon does
        // (`jwks_standard_claims.zig`: `if (exp <= now_s)`), including the
        // boundary: a token expiring exactly now is expired.
        if exp <= now {
            return Err(VerifyError::Expired);
        }
        // Checked when present, never required. The configured provider sends
        // `nbf` on every session token — ten seconds behind `iat` — so this is
        // unreachable against it; but the issuer is a deployment knob, which
        // makes this verifier's contract "an OIDC provider" rather than one
        // vendor, and RFC 7519's not-before rule makes a future `nbf` a
        // refusal. No
        // leeway, for the reason `exp` has none: the session token lives sixty
        // seconds, and a skew allowance large enough to matter is a meaningful
        // fraction of its whole life.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a Unix second count is far inside f64's exact-integer range; the widening exists to accept the non-integer NumericDate RFC 7519 permits"
        )]
        let now_seconds = now as f64;
        if claims.nbf.is_some_and(|nbf| nbf > now_seconds) {
            return Err(VerifyError::NotYetValid);
        }
        // Said out loud, because the refusal alone is not: `OidcFlow::redact`
        // collapses this into the generic rejection every bad token gets, so
        // without a line here a mis-provisioned ceiling is an unexplained
        // stream of 401s. The VALUE is never logged — it is claim content from
        // a token, and naming the claim is what an operator needs.
        let ceiling = claims.ceiling().inspect_err(|_unreadable| {
            tracing::warn!(
                event = "workspace_ceiling_unreadable",
                "a token carries a workspace ceiling this daemon cannot read; \
                 the token is refused rather than served without the confinement"
            );
        });

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
            // Fallible where the tenant is not: an unreadable ceiling refuses
            // the token rather than reading as "no ceiling". See
            // `Claims::ceiling` for why the two claims part company here.
            workspace_scope: ceiling?,
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
        let token = presented.clone();
        async move { self.verify_token(token.expose()).await }
    }
}

/// Verify before parsing claims, then pass every policy check to `read_claims`.
fn verify_claims(key: &SigningKey, segments: Segments<'_>) -> Result<Claims, VerifyError> {
    let signing_input = format!("{}.{}", segments.header, segments.payload);
    let verified = jsonwebtoken::crypto::verify(
        segments.signature,
        signing_input.as_bytes(),
        &key.decoding_key,
        jsonwebtoken::Algorithm::RS256,
    )
    .map_err(|_invalid_encoding| VerifyError::Malformed)?;
    if !verified {
        return Err(VerifyError::SignatureInvalid);
    }

    let payload = decode_segment(segments.payload)?;
    afd_core::json::object_from_slice(&payload).map_err(|_invalid| VerifyError::Malformed)
}
