//! Authenticated GitHub App exchange after scope parsing and verification.
//!
//! Two calls per mint, both under the App's signature: the installation's
//! permissions, then the token asked for within them.

use std::time::Duration;

use afd_fleet_runtime::config::RepositoryBinding;
use jiff::Timestamp;
use serde_json::Value;
use url::Url;

use super::api::{AppApi, Failure, sign};
use super::{Granted, Installed, ScopedRequest, installation};
use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::credential::platform::GithubApp;

/// Where an App reads one of its installations; the token is minted beneath it.
const INSTALLATIONS_ROUTE: &str = "/app/installations";

/// How long an installation token lasts, per GitHub's documentation.
///
/// Bounded locally rather than read from the response's `expires_at`, which is
/// what the runner has always been told. A shorter vendor lifetime would require
/// changing this policy so the broker never caches a dead token.
const INSTALLATION_TOKEN_TTL: Duration = Duration::from_hours(1);

/// Everything one GitHub mint needs.
#[derive(Debug, Clone, Copy)]
pub struct Exchange<'a> {
    /// This deployment's App — the signing key never leaves the process.
    pub app: &'a GithubApp,
    /// The workspace's stored handle, naming which installation to mint for.
    pub handle: &'a Value,
    /// The fleet's declared reach. `None` refuses before anything is signed.
    pub binding: Option<&'a RepositoryBinding>,
    /// The client both calls go through. Shared and passed in, so the broker's
    /// connection pool and timeout are one decision made once.
    pub http: &'a reqwest::Client,
    /// The instant the App JWT is signed at and the token's expiry is measured
    /// from.
    pub now: Timestamp,
}

/// Where GitHub's REST API lives; [`mint_at`] takes it so a test can point the
/// same signed client at a loopback GitHub.
const GITHUB_API: &str = "https://api.github.com";

/// Mints a repository-scoped installation token.
///
/// The binding is required before the JWT is built. Without the body that
/// narrows the token, GitHub would interpret the request as full installation
/// reach, so absence must fail closed before transport.
pub async fn mint(exchange: Exchange<'_>) -> Outcome {
    mint_at(exchange, GITHUB_API).await
}

/// [`mint`] against the GitHub API rooted at `base`.
pub(super) async fn mint_at(exchange: Exchange<'_>, base: &str) -> Outcome {
    let Some(installation_id) = installation(exchange.handle) else {
        return Outcome::ReconnectRequired;
    };
    let Some(binding) = exchange.binding else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    // One refusal for every way the App cannot sign: a key that is not an RSA
    // PEM and an API root that is not a URL are both the operator's to fix, and
    // no retry helps either.
    let Some(api) = Url::parse(base).ok().and_then(|base| {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(exchange.app.private_key_pem.as_bytes())
            .ok()?;
        let bearer = sign(exchange.app.app_id, &key, exchange.now)?;
        Some(AppApi::new(exchange.http, base, bearer))
    }) else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    request_token(&api, installation_id, binding, exchange.now).await
}

/// Reads the installation, then posts the narrowed request, through a
/// supplied client.
pub(super) async fn request_token(
    api: &AppApi<'_>,
    installation_id: u64,
    binding: &RepositoryBinding,
    now: Timestamp,
) -> Outcome {
    let (granted, request) = match scoped_token(api, installation_id, binding).await {
        Ok(answered) => answered,
        Err(failure) => return failure.into(),
    };

    let requested = request.permissions();
    if let Err(overreach) = granted.verify(binding, requested) {
        // Names and levels only, never the token: which permission tripped the
        // check is what an operator needs to tell a new ambient grant from a
        // mis-scoped installation.
        tracing::warn!(
            ?overreach,
            granted = ?granted.permissions,
            ?requested,
            event = "github_mint_overreach",
            "discarding a GitHub token whose reach does not match the fleet's binding"
        );
        return Outcome::MintFailed(Retry::Permanent);
    }

    Outcome::Ok(Minted {
        token: granted.token.into(),
        expires_at: afd_core::clock::after(now, INSTALLATION_TOKEN_TTL),
        rotated_refresh_token: None,
    })
}

/// The installation read and the narrowed token request, in order; the token
/// is asked for only once the installation answered.
async fn scoped_token(
    api: &AppApi<'_>,
    installation_id: u64,
    binding: &RepositoryBinding,
) -> Result<(Granted, ScopedRequest), Failure> {
    let installation = format!("{INSTALLATIONS_ROUTE}/{installation_id}");
    let installed: Installed = api.get(&installation).await?;
    let request = ScopedRequest::for_binding(binding, &installed);
    let granted = api
        .post(&format!("{installation}/access_tokens"), &request)
        .await?;
    Ok((granted, request))
}
