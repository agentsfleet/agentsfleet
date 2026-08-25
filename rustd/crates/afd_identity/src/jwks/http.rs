//! Reading a key set over the network.
//!
//! The port of `auth/jwks_fetch.zig` and `oidc.zig`'s URL resolution, and the
//! only file in this crate that opens a socket.
//!
//! # One code path decides the URL
//!
//! `oidc.zig::resolveJwksUrl` exists because a runtime loader and a `doctor`
//! command that derived the URL separately could test a different endpoint than
//! the daemon fetched — the issuer/key-set drift bug class. [`jwks_url`] is the
//! single resolver here for the same reason.

use afd_auth::verifier::VerifyError;

use crate::jwks::source::{KeySetSource, MAX_REDIRECTS, MAX_RESPONSE_BYTES};

/// Appended to an issuer to form its key-set endpoint.
///
/// The `OpenID` Connect convention for publishing signing keys, and
/// `oidc.zig`'s `WELL_KNOWN_JWKS_SUFFIX`.
const WELL_KNOWN_SUFFIX: &str = "/.well-known/jwks.json";

/// What the body buffer starts at before the capped read grows it.
///
/// A key set is a few kilobytes, so this is one allocation for the common case
/// rather than a bound — `MAX_RESPONSE_BYTES` is the bound.
const INITIAL_BODY_BYTES: usize = 8 * 1024;

/// Whitespace trimmed from a configured issuer or override.
const TRIMMED: [char; 4] = [' ', '\t', '\r', '\n'];

/// Resolves the endpoint to fetch, from an optional override and an issuer.
///
/// An explicit, non-empty override wins and is returned trimmed — a padded
/// value in an environment file is a configuration typo, not a dead URL.
/// Otherwise the issuer is trimmed, stripped of EVERY trailing slash, and given
/// the well-known suffix; a doubled slash in the path 404s at real providers.
///
/// Returns `None` when neither yields a URL, which is how a deployment says it
/// has no identity provider.
#[must_use]
pub fn jwks_url(override_url: Option<&str>, issuer: Option<&str>) -> Option<String> {
    if let Some(explicit) = override_url.map(|raw| raw.trim_matches(TRIMMED))
        && !explicit.is_empty()
    {
        return Some(explicit.to_owned());
    }
    let base = issuer?.trim_matches(TRIMMED).trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    Some(format!("{base}{WELL_KNOWN_SUFFIX}"))
}

/// Fetches a key set over HTTPS.
#[derive(Debug)]
pub struct HttpKeySet {
    client: reqwest::Client,
    url: String,
}

impl HttpKeySet {
    /// Builds a fetcher for `url`.
    ///
    /// # Errors
    /// [`VerifyError::KeySetUnavailable`] when a client cannot be constructed.
    /// The provider is not among the reasons: this crate selects ring at
    /// compile time (see the `reqwest` note in the workspace manifest), so
    /// there is no process-default to be missing.
    pub fn new(url: impl Into<String>, timeout: std::time::Duration) -> Result<Self, VerifyError> {
        let client = reqwest::Client::builder()
            // Every deadline in this workspace is at the call site (Invariant
            // 4); this is the call site.
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            // The identity provider is reached directly. An environment proxy
            // silently rerouting the fetch that establishes who may act is not
            // a convenience worth having.
            .no_proxy()
            .build()
            .map_err(|_unbuildable| VerifyError::KeySetUnavailable)?;
        Ok(Self {
            client,
            url: url.into(),
        })
    }

    /// The endpoint this fetcher reads, for a boot diagnostic.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Reads the body, refusing past the cap.
    ///
    /// Streamed rather than `bytes()`: a `Content-Length` is the server's
    /// claim about itself, and this daemon bounds what it actually reads. No
    /// content-encoding is ever negotiated (see the `reqwest` note in the
    /// workspace manifest), so these bytes are the decoded bytes and the cap
    /// means what `jwks_fetch.zig` says it means.
    async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, VerifyError> {
        let mut body = Vec::with_capacity(INITIAL_BODY_BYTES);
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|_transport| VerifyError::KeySetUnavailable)?;
            let Some(chunk) = chunk else { break };
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                let cap = MAX_RESPONSE_BYTES;
                tracing::warn!(
                    cap,
                    event = "jwks_response_too_large",
                    "refusing the key set"
                );
                return Err(VerifyError::KeySetUnavailable);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}
impl KeySetSource for HttpKeySet {
    async fn fetch(&self) -> Result<Vec<u8>, VerifyError> {
        let response = self.client.get(&self.url).send().await.map_err(|err| {
            // Hoisted out of the macro: `tracing`'s `log` feature is on across
            // this workspace, so a call inside an event field compiles twice
            // and llvm-cov reports the dead copy.
            let cause = err.to_string();
            let url = self.url.clone();
            tracing::warn!(url, cause, event = "jwks_fetch_failed");
            VerifyError::KeySetUnavailable
        })?;
        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            let url = self.url.clone();
            tracing::warn!(url, code, event = "jwks_fetch_rejected");
            return Err(VerifyError::KeySetUnavailable);
        }
        Self::read_capped(response).await
    }
}
