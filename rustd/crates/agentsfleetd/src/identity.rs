//! The identity provider this deployment has, or the refusal it has instead.
//!
//! Two seams — what a person may do, and whether a session token is genuine —
//! and both are reachable only when an operator has configured a provider. The
//! Zig daemon spells that as `verifier: ?*oidc.Verifier` plus an `orelse` at
//! each call site, which is an optional every reader has to trace to find out
//! what an unset knob actually does.
//!
//! Here each is an enum with two variants, and the answer is the variant's own
//! body. An unconfigured provider is an OUTAGE — `UZ-AUTH-004`, 503 — never an
//! empty capability set: an empty set would authenticate a caller and then
//! refuse them at every gate as though they had been narrowed to nothing, which
//! is indistinguishable from a real demotion. `clerk_scope_resolver.zig` makes
//! the same choice by treating an absent secret as a fetch failure.
//!
//! # What the runner plane consults: neither seam
//!
//! A runner token resolves through Postgres and derives its capabilities from
//! the credential class, so neither seam is read on any `/v1/runners/me`
//! request. That is why an unbuildable provider degrades the tenant surface
//! alone and never the runner plane. It is NOT a licence to boot without one:
//! `preflight` requires the provider knobs the way `runtime_validate.zig`
//! does, so the unconfigured variants below are reachable only from a
//! construction failure, never from an operator who set nothing.

use std::sync::Arc;
use std::time::Duration;

use afd_auth::capability::{CapabilitySource, NoCapabilitySource};
use afd_auth::credential::Presented;
use afd_auth::error::Unavailable;
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_auth::verifier::{NoVerifier, TokenVerifier, VerifiedClaims, VerifyError};
use afd_identity::{
    HttpKeySet, JwksVerifier, ProviderCapabilities, ProviderClaims, VerifierConfig, jwks_url,
};

use crate::preflight::IdentityConfig;

/// How long the provider has to answer a capability read.
///
/// Shorter than any request budget it sits inside: a gate that waits longer
/// than the caller will is a gate that turns a slow provider into a timeout the
/// caller reports instead.
pub const PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the key-set endpoint has to answer.
pub const KEY_SET_TIMEOUT: Duration = Duration::from_secs(5);

/// What a subject may do, if anyone can be asked.
#[derive(Debug, Clone)]
pub enum Capabilities {
    /// A configured provider, behind the documented freshness windows.
    Provider(ProviderCapabilities<ProviderClaims>),
    /// No provider. Every capability read is an outage, by design.
    Unconfigured(NoCapabilitySource),
}

impl CapabilitySource for Capabilities {
    async fn capabilities(&self, subject: &Subject) -> Result<ScopeSet, Unavailable> {
        match self {
            Self::Provider(provider) => provider.capabilities(subject).await,
            Self::Unconfigured(absent) => absent.capabilities(subject).await,
        }
    }
}

/// Whether a session token is genuine, if anything can check.
///
/// The verifier is shared rather than cloned: it owns a key-set cache, and two
/// copies would be two caches fetching on their own schedules.
#[derive(Debug, Clone)]
pub enum Sessions {
    /// A configured issuer, verified against its published key set.
    Jwks(Arc<JwksVerifier<HttpKeySet>>),
    /// No issuer. A session token is refused; the marked classes still resolve.
    Unconfigured(NoVerifier),
}

impl TokenVerifier for Sessions {
    async fn verify(&self, presented: &Presented) -> Result<VerifiedClaims, VerifyError> {
        match self {
            Self::Jwks(verifier) => verifier.verify(presented).await,
            Self::Unconfigured(absent) => absent.verify(presented).await,
        }
    }
}

/// Builds both seams from what an operator configured.
///
/// Answers the unconfigured pair when a configured provider cannot be
/// CONSTRUCTED — a client builder that fails, or an issuer no key-set URL can
/// be derived from. That is deliberately not a boot refusal: the failure is a
/// client, not a credential, so it says the tenant surface is unavailable
/// rather than taking a healthy runner plane down with it. Preflight has
/// already refused the boot if a knob was missing, so an absent provider can no
/// longer reach this function at all.
#[must_use]
pub fn resolve(identity: &IdentityConfig) -> (Capabilities, Sessions) {
    (capabilities(identity), sessions(identity))
}

/// The capability seam for a configured provider.
fn capabilities(identity: &IdentityConfig) -> Capabilities {
    match ProviderClaims::new(
        identity.api_base.clone(),
        identity.secret.clone(),
        PROVIDER_TIMEOUT,
    ) {
        Ok(claims) => Capabilities::Provider(ProviderCapabilities::new(
            claims,
            Arc::new(afd_core::clock::SystemClock),
        )),
        Err(_unbuildable) => {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the dead copy.
            let code = afd_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::error!(
                error_code = code,
                "no HTTP client for the identity provider — every tenant-plane \
                 capability read will answer unavailable"
            );
            Capabilities::Unconfigured(NoCapabilitySource)
        }
    }
}

/// The session seam for a configured issuer.
fn sessions(identity: &IdentityConfig) -> Sessions {
    // The key-set URL is DERIVED from the issuer unless overridden, so the two
    // can never name different providers — the property `jwks_url` exists for.
    let Some(url) = jwks_url(identity.jwks_url.as_deref(), Some(&identity.issuer)) else {
        return Sessions::Unconfigured(NoVerifier);
    };
    match HttpKeySet::new(url, KEY_SET_TIMEOUT) {
        Ok(key_set) => Sessions::Jwks(Arc::new(JwksVerifier::new(
            key_set,
            VerifierConfig::new(identity.issuer.clone(), identity.audience.clone()),
            Arc::new(afd_core::clock::SystemClock),
        ))),
        Err(_unbuildable) => {
            let code = afd_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::error!(
                error_code = code,
                "no HTTP client for the key-set endpoint — every session token \
                 will answer unavailable"
            );
            Sessions::Unconfigured(NoVerifier)
        }
    }
}
