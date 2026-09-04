//! Asking the identity provider what a person may do.
//!
//! The port of `auth/clerk_scope_fetch.zig`. It is the only thing under
//! [`crate::capability`] that opens a socket, and it is separate from the cache
//! for that reason: the cache's three windows are provable without a provider,
//! and this is provable without a cache.
//!
//! # Every shape that is not a present string grants nothing
//!
//! `public_metadata` is hand-edited by an operator. A missing object, a
//! `scopes` key holding a number, a `null` — each one narrows the principal to
//! no capabilities rather than failing the request open. That is the direction
//! this must fail in, and it is the same rule
//! [`afd_auth::scope::parse_claim`] applies to an unknown token inside a claim.

use afd_auth::principal::Subject;

use crate::capability::ClaimSource;
use crate::error::{ClaimUnavailable, Error, Result};

/// The provider's backend API secret.
///
/// A newtype rather than a `String` because of what it is: the credential that
/// lets this daemon read EVERY person's capabilities. It is held for the
/// process's life inside a service the boot path prints, so a derived `Debug`
/// anywhere up the ownership chain would put it in a log (Invariant 5).
///
/// `afd_crypto`'s `SecretBytes` is the workspace's byte-shaped equivalent; this
/// is the string-shaped one, kept local rather than pulling a crypto crate into
/// this graph for a header value.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderSecret(Box<str>);

impl ProviderSecret {
    /// Wraps the boot-resolved secret.
    ///
    /// # Errors
    /// Returns [`Error::BlankSecret`] when the value is empty or only whitespace.
    /// `clerk_scope_fetch.zig` treats an absent or blank secret as
    /// `MissingSecret` for the same reason: capabilities cannot resolve at all
    /// without it, which is an outage rather than an empty grant, and saying so
    /// at boot beats discovering it on the first authenticated request.
    pub fn new(raw: &str) -> Result<Self> {
        if raw.trim().is_empty() {
            return Err(Error::BlankSecret);
        }
        Ok(Self(raw.into()))
    }

    /// The secret, for the one header that carries it.
    ///
    /// `pub(crate)` rather than private: the write side lives in
    /// [`crate::metadata`] and carries the same credential in the same header.
    #[must_use]
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

/// Renders nothing.
impl std::fmt::Debug for ProviderSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProviderSecret(redacted)")
    }
}

/// Zeroes the secret when the process drops it.
impl Drop for ProviderSecret {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        let mut bytes = std::mem::take(&mut self.0).into_boxed_bytes();
        bytes.zeroize();
    }
}

/// Upper bound on a user document, in bytes.
///
/// `clerk_scope_fetch.zig`'s `USER_MAX_RESPONSE_BYTES`. Smaller than the key
/// set's cap because a user document is smaller, and the bound exists for the
/// same reason: the response is not this daemon's to trust.
pub const USER_MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// The claim a subject resolves to when the provider knows them and an operator
/// has provisioned nothing.
///
/// Empty, not absent, and it is cached like any other answer: an operator who
/// has granted nothing has made a decision, and re-asking on every request
/// would spend a round trip to be told the same thing.
pub const UNPROVISIONED_CLAIM: &str = "";

/// The media type both directions of the provider API speak.
///
/// Bound rather than spelled twice: the read sends it as `Accept`, the write
/// as `Content-Type` (RULE UFS).
pub(crate) const JSON_MEDIA_TYPE: &str = "application/json";

/// The object an operator writes capabilities into.
const PUBLIC_METADATA_KEY: &str = "public_metadata";

/// What the body buffer starts at before the capped read grows it.
///
/// Smaller than the JWKS buffer because a claim response is smaller than a key
/// set. `USER_MAX_RESPONSE_BYTES` is the bound; this is only the first
/// allocation.
const INITIAL_BODY_BYTES: usize = 4 * 1024;
/// The key inside it.
const SCOPES_KEY: &str = "scopes";

/// Reads capability claims from the identity provider's backend API.
#[derive(Debug)]
pub struct ProviderClaims {
    client: reqwest::Client,
    api_base: Box<str>,
    secret: ProviderSecret,
}

impl ProviderClaims {
    /// Builds a claim reader against `api_base`.
    ///
    /// # Errors
    /// [`ClaimUnavailable::Unreachable`] when a client cannot be constructed.
    ///
    pub fn new(
        api_base: impl Into<Box<str>>,
        secret: ProviderSecret,
        timeout: std::time::Duration,
    ) -> Result<Self, ClaimUnavailable> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            // The provider is reached directly. An environment proxy silently
            // rerouting the call that decides what a person may do is not a
            // convenience worth having.
            .no_proxy()
            .build()
            .map_err(|_unbuildable| ClaimUnavailable::Unreachable)?;
        Ok(Self {
            client,
            api_base: api_base.into(),
            secret,
        })
    }

    /// Pulls `public_metadata.scopes` out of a user document.
    ///
    /// # Errors
    /// [`ClaimUnavailable::Unreachable`] when the body is not a JSON object at
    /// all — a provider answering 200 with something that is not a user is not
    /// evidence about the person.
    fn extract_claim(body: &[u8]) -> Result<String, ClaimUnavailable> {
        let document: serde_json::Value = afd_core::json::object_from_slice(body)
            .map_err(|_invalid| ClaimUnavailable::Unreachable)?;
        if !document.is_object() {
            return Err(ClaimUnavailable::Unreachable);
        }
        // Every shape that is not a present string is the unprovisioned claim,
        // so a hand-edited metadata object narrows a principal rather than
        // failing a request open.
        Ok(document
            .get(PUBLIC_METADATA_KEY)
            .and_then(|metadata| metadata.get(SCOPES_KEY))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(UNPROVISIONED_CLAIM)
            .to_owned())
    }

    /// Maps a response status onto the two outcomes a caller acts on.
    ///
    /// A 404 is deliberately distinct: it says the person is GONE, which is
    /// permanent, while everything else says this daemon could not find out,
    /// which is not. `clerk_scope_fetch.zig::mapStatus` draws the same line,
    /// and it is extracted here for the same reason — every branch is provable
    /// without standing up a listener.
    #[must_use]
    pub const fn classify(status: u16) -> Option<ClaimUnavailable> {
        match status {
            200..=299 => None,
            404 => Some(ClaimUnavailable::UnknownSubject),
            _ => Some(ClaimUnavailable::Unreachable),
        }
    }

    /// Reads the body, refusing past the cap.
    async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, ClaimUnavailable> {
        let mut body = Vec::with_capacity(INITIAL_BODY_BYTES);
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|_transport| ClaimUnavailable::Unreachable)?;
            let Some(chunk) = chunk else { break };
            if body.len().saturating_add(chunk.len()) > USER_MAX_RESPONSE_BYTES {
                let cap = USER_MAX_RESPONSE_BYTES;
                tracing::warn!(
                    cap,
                    event = "scope_response_too_large",
                    "refusing the user document"
                );
                return Err(ClaimUnavailable::Unreachable);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

impl ClaimSource for ProviderClaims {
    async fn claim(&self, subject: &Subject) -> Result<String, ClaimUnavailable> {
        let url = format!("{}/users/{}", self.api_base, subject.as_str());
        let response = self
            .client
            .get(&url)
            .bearer_auth(self.secret.expose())
            .header(reqwest::header::ACCEPT, JSON_MEDIA_TYPE)
            .send()
            .await
            .inspect_err(|failure| {
                // Hoisted out of the macro: `tracing`'s `log` feature is on
                // across this workspace, so a call inside an event field
                // compiles twice and llvm-cov reports the dead copy.
                //
                // The URL is NOT logged: it carries the subject, and a log line
                // naming who was being resolved during an outage is a record of
                // who was active.
                let cause = failure.to_string();
                tracing::warn!(cause, event = "scope_fetch_failed");
            })
            // `inspect_err` observes, `map_err` maps. Splitting them keeps the
            // cause out of the error VALUE: this one carries no source, so
            // stringifying into it would have dropped the chain (RULE ERR-RS).
            .map_err(|_logged| ClaimUnavailable::Unreachable)?;

        if let Some(refusal) = Self::classify(response.status().as_u16()) {
            let code = response.status().as_u16();
            tracing::warn!(code, event = "scope_fetch_rejected");
            return Err(refusal);
        }
        let body = Self::read_capped(response).await?;
        Self::extract_claim(&body)
    }
}
