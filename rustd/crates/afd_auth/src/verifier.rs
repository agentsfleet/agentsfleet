//! Verifying a signed session token, behind a seam.
//!
//! The token path differs from every other class in one way that shapes this
//! module: the capability claim rides ON the token. There is no second question
//! to ask, so [`VerifiedClaims`] carries the claim string and the OIDC path
//! never consults a [`crate::capability::CapabilitySource`].
//!
//! # Why the errors are a different type from `Error`
//!
//! A verifier reports what went wrong with a token; the authenticator decides
//! what a caller is told. Those are different jobs, and collapsing them would
//! put the "which failure leaks what" decision inside every verifier
//! implementation instead of in one place. [`VerifyError`] is the honest
//! account; [`crate::error::Error`] is the redacted one, and
//! `OidcFlow` is the single boundary between them.
//!
//! Two mappings there are worth naming, because both are Zig parity:
//!
//! - **Expiry survives** as its own code. It leaks nothing — the holder already
//!   proved possession of a validly-signed token — and the remedy differs.
//! - **A key-set failure becomes `Unavailable`, not a rejection.**
//!   `bearer_or_api_key.zig:99-102` maps `JwksFetchFailed` and `JwksParseFailed`
//!   to `ERR_AUTH_UNAVAILABLE`, because a provider outage is not evidence about
//!   the caller's token.

use crate::credential::Presented;

/// What a token said, once its signature and standard claims checked out.
///
/// Only the fields this daemon acts on. `bearer_or_api_key.zig` frees the rest
/// (`issuer`, `org_id`, `audience`) immediately after verification, under a
/// comment explaining they would otherwise leak; here they are simply never
/// constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    /// The `sub` claim — the provider's identifier for the person.
    pub subject: crate::principal::Subject,
    /// The tenant claim, absent on a token minted before a tenant exists.
    pub tenant: Option<afd_core::id::Uuid7>,
    /// The `workspace_id` claim, the only ceiling any credential can carry.
    pub workspace_scope: Option<afd_core::id::Uuid7>,
    /// The raw space-delimited `scopes` claim, absent when the token carries
    /// none. Left unparsed so [`crate::scope::parse_claim`] stays the single
    /// place a claim becomes a capability set.
    pub scope_claim: Option<Box<str>>,
}

/// Why a token was not accepted.
///
/// Finer-grained than what a client is told, on purpose — see the module note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// The value is not three base64url segments separated by dots.
    #[error("token is not a well-formed JWT")]
    Malformed,
    /// The header names an algorithm this daemon does not accept.
    ///
    /// Only `RS256`. Notably this refuses `none`, the algorithm-confusion
    /// attack, by refusing everything that is not the one expected value rather
    /// than by blocklisting the dangerous ones.
    #[error("unsupported signing algorithm")]
    UnsupportedAlgorithm,
    /// The header carries no `kid`, so no key can be selected.
    #[error("token header carries no key id")]
    MissingKeyId,
    /// The key set has no key under that `kid`, after a refresh.
    #[error("no signing key matches the token's key id")]
    KeyNotFound,
    /// The signature does not verify against the selected key.
    #[error("token signature is invalid")]
    SignatureInvalid,
    /// A required standard claim is absent.
    #[error("token is missing a required claim")]
    MissingClaim,
    /// The `iss` claim is not the configured issuer.
    #[error("token issuer does not match")]
    IssuerMismatch,
    /// The `aud` claim does not contain the configured audience.
    #[error("token audience does not match")]
    AudienceMismatch,
    /// The `exp` claim has passed.
    #[error("token expired")]
    Expired,
    /// The key set could not be fetched or parsed.
    ///
    /// Not evidence about the token. Becomes `UZ-AUTH-004`.
    #[error("signing key set is unavailable")]
    KeySetUnavailable,
    /// This deployment has no identity provider configured.
    ///
    /// A REJECTION, not an outage, and that is Zig parity:
    /// `bearer_or_api_key.zig:95-98` answers `ERR_UNAUTHORIZED` for
    /// `self.verifier orelse`. An operator who never configured an issuer has
    /// not suffered an outage, and telling a caller to retry would be a lie.
    #[error("no identity provider is configured")]
    NotConfigured,
}

impl VerifyError {
    /// Whether this failure describes the PROVIDER rather than the token.
    ///
    /// The one bit the authenticator needs in order to answer `UZ-AUTH-004`
    /// instead of a rejection, and it lives here so a new variant has to
    /// declare which side it falls on.
    #[must_use]
    pub const fn is_provider_fault(self) -> bool {
        matches!(self, Self::KeySetUnavailable)
    }
}

/// Verifies a signed session token against a key set.
///
/// # Errors
/// [`VerifyError`], which the caller redacts before it reaches a client.
///
/// # Design
///
/// One method, per `M-DI-HIERARCHY`. The implementation that fetches a real
/// key set lives in `afd_identity`, so this crate stays free of an HTTP client
/// and its branches stay provable with fixtures.
pub trait TokenVerifier: Send + Sync + std::fmt::Debug {
    /// Verifies the token and returns what it claimed.
    ///
    /// # Errors
    /// [`VerifyError`] describing either the token or the key set.
    fn verify(
        &self,
        presented: &Presented,
    ) -> impl Future<Output = Result<VerifiedClaims, VerifyError>> + Send;
}

/// The verifier a deployment with no identity provider holds.
///
/// Not a stub and not a test double — it is what `OIDC_ISSUER` being unset
/// MEANS, expressed as a type. The Zig daemon spells the same thing as
/// `verifier: ?*oidc.Verifier` and an `orelse` at the one call site, which is
/// an optional every reader has to trace to find out what happens when it is
/// null. Here the answer is the type's whole body.
///
/// It is also what makes Dimension 4.1's second half structural rather than
/// procedural: with no verifier configured, an `agt_t` or `afc_` credential is
/// still resolved, because those classes never consult a verifier at all. In
/// the Zig daemon that holds because two `if`s sit above the `orelse`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVerifier;

impl TokenVerifier for NoVerifier {
    fn verify(
        &self,
        _presented: &Presented,
    ) -> impl Future<Output = Result<VerifiedClaims, VerifyError>> + Send {
        std::future::ready(Err(VerifyError::NotConfigured))
    }
}
