//! Authenticated GitHub App exchange after scope parsing and verification.

use afd_fleet_runtime::config::RepositoryBinding;
use octocrab::Octocrab;
use octocrab::models::AppId;
use serde_json::Value;

use super::{Granted, ScopedRequest, classify, installation_id};
use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::credential::platform::GithubApp;

const FIELD_INSTALLATION_ID: &str = "installation_id";

/// How long an installation token lasts, per GitHub's documentation.
///
/// Bounded locally rather than read from the response's `expires_at`, matching
/// the existing integration contract. A shorter vendor lifetime would require
/// changing this policy so the broker never caches a dead token.
const INSTALLATION_TOKEN_TTL_MS: i64 = 60 * 60 * 1000;

/// Everything one GitHub mint needs.
#[derive(Debug, Clone, Copy)]
pub struct Exchange<'a> {
    /// This deployment's App — the signing key never leaves the process.
    pub app: &'a GithubApp,
    /// The workspace's stored handle, naming which installation to mint for.
    pub handle: &'a Value,
    /// The fleet's declared reach. `None` refuses before anything is signed.
    pub binding: Option<&'a RepositoryBinding>,
    /// The instant the token's expiry is measured from.
    pub now_ms: i64,
}

/// Mints a repository-scoped installation token.
///
/// The binding is required before the JWT is built. Without the body that
/// narrows the token, GitHub would interpret the request as full installation
/// reach, so absence must fail closed before transport.
pub async fn mint(exchange: Exchange<'_>) -> Outcome {
    let Some(installation_id) = exchange
        .handle
        .as_object()
        .and_then(|handle| handle.get(FIELD_INSTALLATION_ID))
        .and_then(installation_id)
    else {
        return Outcome::ReconnectRequired;
    };
    let Some(binding) = exchange.binding else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    let key = match jsonwebtoken::EncodingKey::from_rsa_pem(exchange.app.private_key_pem.as_bytes())
    {
        Ok(key) => key,
        Err(_unusable) => return Outcome::MintFailed(Retry::Permanent),
    };
    let Ok(client) = Octocrab::builder()
        .app(AppId(exchange.app.app_id), key)
        .build()
    else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    request_token(&client, installation_id, binding, exchange.now_ms).await
}

/// Posts the narrowed request through a supplied client.
pub(super) async fn request_token(
    client: &Octocrab,
    installation_id: u64,
    binding: &RepositoryBinding,
    now_ms: i64,
) -> Outcome {
    let request = ScopedRequest::for_binding(binding);
    let granted: Granted = match client
        .post(
            format!("/app/installations/{installation_id}/access_tokens"),
            Some(&request),
        )
        .await
    {
        Ok(granted) => granted,
        Err(error) => return classify(&error),
    };

    if let Err(overreach) = granted.verify(binding, request.permissions()) {
        tracing::warn!(
            ?overreach,
            event = "github_mint_overreach",
            "discarding a GitHub token whose reach does not match the fleet's binding"
        );
        return Outcome::MintFailed(Retry::Permanent);
    }

    Outcome::Ok(Minted {
        token: granted.token.into(),
        expires_at_ms: now_ms.saturating_add(INSTALLATION_TOKEN_TTL_MS),
        rotated_refresh_token: None,
    })
}
