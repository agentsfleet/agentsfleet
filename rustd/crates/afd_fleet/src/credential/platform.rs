//! The PLATFORM's own credentials — this deployment's App and OAuth clients.
//!
//! Distinct from a workspace's stored handle, and the distinction is the whole
//! security model of the mint. A handle says WHICH installation or account a
//! tenant connected; these say who WE are to the vendor. The handle is a tenant
//! secret and it is per-workspace; these are one deployment's and never leave
//! this process — only the minted token crosses back to a runner.
//!
//! # Keyed by connector name, not by named fields
//!
//! `integration_ctx.zig` declares `PlatformSecrets { github, zoho, jira, linear }`
//! as four named fields, and pays for it three times over in
//! `integration.zig`: `selectZoho`, `selectJira` and `selectLinear` are three
//! near-identical functions whose entire body is `return p.zoho;`. They exist
//! only because a struct field cannot be looked up by a string, so every
//! connector needs a function to reach its own row — and a fifth provider needs
//! a fifth one.
//!
//! A map keyed by the connector's declared name needs none of them. Adding a
//! provider is a row in `connector::DECLARED` and a secret in the vault, and
//! this file does not change at all.

use std::collections::BTreeMap;

use afd_core::id::Uuid7;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::secrets::connector::{Connector as _, Exchange, Registry};
use crate::vault::{KeyRef, Vault};

/// This deployment's GitHub App.
///
/// The App key never leaves the daemon — it signs a JWT here, and only the
/// installation token it buys is handed to a runner (RULE VLT).
#[derive(Clone)]
pub struct GithubApp {
    /// The numeric App id, as GitHub issues it.
    pub app_id: u64,
    /// The App's RSA private key, PEM-encoded.
    ///
    /// [`Zeroizing`] so the bytes are wiped when the last handle drops rather
    /// than left in freed memory for whatever allocates next.
    pub private_key_pem: Zeroizing<String>,
}

// Hand-written, because deriving it would print the key. There is no field here
// worth showing and one that must never be shown, so the whole value renders as
// its type — `missing_debug_implementations` is satisfied and nothing leaks.
impl std::fmt::Debug for GithubApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubApp")
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

/// This deployment's OAuth client with one refresh-grant provider.
#[derive(Clone)]
pub struct OauthApp {
    /// The public half.
    pub client_id: String,
    /// The half that authenticates the refresh grant.
    pub client_secret: Zeroizing<String>,
}

// Hand-written for the reason [`GithubApp`]'s is: `client_secret` must never
// reach a log line, and a derived `Debug` would put it there.
impl std::fmt::Debug for OauthApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OauthApp")
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

/// Every platform credential this deployment holds, by connector name.
///
/// Absence is ordinary rather than exceptional: a deployment that connected
/// GitHub and nothing else holds one entry, and a mint for anything else
/// refuses because the platform never configured it — not because the tenant
/// did anything wrong.
#[derive(Debug, Clone, Default)]
pub struct Platform {
    /// The GitHub App, if this deployment has one. Its own field rather than a
    /// map entry because it is a different SHAPE — an App id and a signing key
    /// against a client id and secret — and a map holding both would be a map
    /// of an enum every reader has to unwrap.
    github: Option<GithubApp>,
    /// The OAuth clients, keyed by the connector name that declares them.
    oauth: BTreeMap<String, OauthApp>,
}

impl Platform {
    /// A deployment holding nothing. Every mint refuses.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Records this deployment's GitHub App.
    #[must_use]
    pub fn with_github(mut self, app: GithubApp) -> Self {
        self.github = Some(app);
        self
    }

    /// Records the OAuth client for the connector called `connector`.
    ///
    /// Takes the name as a parameter rather than having a method per provider,
    /// which is the whole point of this module — see the header.
    #[must_use]
    pub fn with_oauth(mut self, connector: &str, app: OauthApp) -> Self {
        self.oauth.insert(connector.to_owned(), app);
        self
    }

    /// The GitHub App, if configured.
    #[must_use]
    pub const fn github(&self) -> Option<&GithubApp> {
        self.github.as_ref()
    }

    /// The OAuth client for `connector`, if configured.
    #[must_use]
    pub fn oauth(&self, connector: &str) -> Option<&OauthApp> {
        self.oauth.get(connector)
    }
}

/// The vault key one connector's platform credential is held under.
///
/// `github-app`, `zoho-app`, and so on — the connector's own declared name plus
/// a fixed stem, which is `serve_broker.zig`'s rule and the reason adding a
/// connector adds a vault ROW rather than an environment knob. Two deployments
/// of this product hold different apps; neither holds a different config shape.
const APP_KEY_STEM: &str = "-app";

/// A GitHub App as its vault row states it.
///
/// The id is read as a STRING because that is what the connect callback wrote,
/// and parsed here — a row holding `"7"` and a row holding `7` are the same App
/// to an operator, and refusing one of them would be refusing a row this
/// product itself produced.
#[derive(Debug, Deserialize)]
struct StoredGithubApp {
    /// The numeric App id, however the row spells it.
    app_id: String,
    /// The App's RSA private key.
    private_key_pem: String,
}

/// An OAuth client as its vault row states it.
#[derive(Debug, Deserialize)]
struct StoredOauthApp {
    /// The public half.
    client_id: String,
    /// The half that authenticates the refresh grant.
    client_secret: String,
}

impl Platform {
    /// Reads every platform credential this deployment holds.
    ///
    /// # Degrading is the whole contract
    ///
    /// Nothing here fails and nothing here refuses boot. A vault row that is
    /// absent, unreadable, or missing a field leaves that connector
    /// unconfigured, and a mint for it answers `UZ-CRED-002` — one endpoint
    /// refusing, rather than a daemon that will not start. `serve_broker.zig`
    /// degrades identically, and the reason is the same: a deployment that
    /// connected only GitHub is an ordinary deployment, not a broken one.
    ///
    /// The connector set is WALKED rather than named, so a connector added to
    /// the registry is loaded here with no edit — see [`Registry::declared`].
    pub async fn load(vault: &Vault, admin_workspace: &Uuid7) -> Self {
        let mut platform = Self::empty();
        for declared in Registry.declared() {
            let name = declared.name();
            let key = format!("{name}{APP_KEY_STEM}");
            let Some(row) = read(vault, admin_workspace, &key).await else {
                continue;
            };
            match declared.exchange() {
                Exchange::GithubApp => {
                    if let Some(app) = github_app(&row) {
                        platform = platform.with_github(app);
                    } else {
                        unusable(&key);
                    }
                }
                Exchange::OAuthRefresh { .. } => {
                    if let Some(app) = oauth_app(&row) {
                        platform = platform.with_oauth(name, app);
                    } else {
                        unusable(&key);
                    }
                }
                // Nothing to exchange, so nothing for this deployment to hold.
                Exchange::Stored => {}
            }
        }
        platform
    }
}

/// One platform credential's stored bytes, or nothing readable.
async fn read(vault: &Vault, admin_workspace: &Uuid7, key: &str) -> Option<Vec<u8>> {
    let held = vault
        .open(KeyRef {
            workspace_id: admin_workspace,
            name: key,
        })
        .await
        .inspect_err(|error| {
            tracing::warn!(
                key,
                error = %error,
                event = "platform_credential_unreadable",
                "a platform credential row would not open; that connector will not mint"
            );
        })
        .ok()??;
    Some(held.expose().to_vec())
}

/// The App a row describes, if it describes one completely.
fn github_app(row: &[u8]) -> Option<GithubApp> {
    let stored: StoredGithubApp = serde_json::from_slice(row).ok()?;
    Some(GithubApp {
        app_id: stored.app_id.parse().ok()?,
        private_key_pem: Zeroizing::new(stored.private_key_pem),
    })
}

/// The OAuth client a row describes, if it describes one completely.
fn oauth_app(row: &[u8]) -> Option<OauthApp> {
    let stored: StoredOauthApp = serde_json::from_slice(row).ok()?;
    Some(OauthApp {
        client_id: stored.client_id,
        client_secret: Zeroizing::new(stored.client_secret),
    })
}

/// Reports a row that exists and cannot be used.
///
/// Worth a line where an absent row is not: an operator who stored a credential
/// and mistyped a field has no other way to find out, and the mint that fails
/// later says only that this deployment is not set up.
fn unusable(key: &str) {
    tracing::warn!(
        key,
        event = "platform_credential_incomplete",
        "a platform credential row is missing a field; that connector will not mint"
    );
}

#[cfg(test)]
mod tests;
