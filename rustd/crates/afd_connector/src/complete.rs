//! Redeeming the code and landing the grant — the second half of a connect.
//!
//! # One exchange, five handles
//!
//! The redeem is provider-agnostic: an authorization code posted to whichever
//! endpoint issued it. What differs is what the answer MEANS, and that is the
//! per-provider parse — Slack's install envelope, the ordinary refresh triple,
//! and the two extras that ride it. `oauth_refresh.zig` splits the same way and
//! for the same reason: a new refresh connector should be a small delta rather
//! than a copied file.
//!
//! # Jira pays for a second round trip, and it is not optional
//!
//! Atlassian's token names no site, and every later API path is built from a
//! cloud id. Landing the grant without one stores a credential with nowhere to
//! spend it — see [`crate::jira`]. The call happens BEFORE the vault write, so
//! a site listing that fails leaves nothing half-connected.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use serde_json::{Map, Value};

use crate::connect::{Connectors, Spent};
use crate::error::{self, Result};
use crate::exchange::Exchanged;
use crate::grant::{Grant, parse};
use crate::provider::Provider;
use crate::registry::Archetype;
use crate::{jira, zoho};

/// The handle field naming the accounts server a Zoho refresh mints at.
const HANDLE_ACCOUNTS_BASE: &str = "accounts_base";

/// The exchange field naming the API host a Zoho grant reaches.
///
/// Kept as an operator-facing LABEL only. The accounts server a refresh is
/// minted at comes from the callback's own `location` — the same signal the
/// exchange itself was routed by — and never from this, which names a related
/// but different host.
const WIRE_API_DOMAIN: &str = "api_domain";

/// What one connect is being finished with.
///
/// A struct rather than six positional arguments, for the reason
/// [`crate::connect::Starting`] is one: `code` and `redirect_uri` are both
/// `&str`, and a transposition posts the callback URL as the authorization
/// code — which the provider refuses with a sentence nobody can act on.
#[derive(Debug, Clone, Copy)]
pub struct Finishing<'f> {
    /// The workspace holding this deployment's own platform credentials.
    pub admin: &'f Uuid7,
    /// What is being connected to.
    pub provider: Provider,
    /// The receipt proving the state was verified, checked and consumed.
    pub spent: &'f Spent,
    /// The authorization code the provider sent back.
    pub code: &'f str,
    /// Which data centre issued it, for the one provider that has several.
    pub location: Option<&'f str>,
    /// The callback URI the code was minted against, echoed exactly.
    pub redirect_uri: &'f str,
}

/// What a completed connect answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// The grant is sealed and whatever it routes is routed.
    Connected,
    /// This deployment has configured no app for the provider.
    NotConfigured,
}

impl Connectors {
    /// Redeems `code` and lands the grant in the workspace the state named.
    ///
    /// Takes a [`Spent`] rather than a workspace: the receipt is what proves
    /// the state was verified, checked against its starter, and consumed — see
    /// [`crate::connect`] on why that ordering is a type.
    ///
    /// # Errors
    /// Reports a provider that could not be reached, one that answered and
    /// refused, an answer carrying no readable grant, and a vault or datastore
    /// that would not take the write.
    pub async fn finish(&self, finishing: Finishing<'_>, now: UnixMillis) -> Result<Landed> {
        let Finishing {
            admin,
            provider,
            spent,
            code,
            location,
            redirect_uri,
        } = finishing;
        let Some(credentials) = self.app.credentials(admin, provider).await? else {
            return Ok(Landed::NotConfigured);
        };

        let endpoint = token_endpoint(provider, location);
        let exchanged = self
            .exchange
            .redeem(provider, &endpoint, &credentials, code, redirect_uri)
            .await?;

        let grant = self.read(provider, &exchanged, location, now).await?;
        self.grants
            .land(spent.workspace(), provider, &grant, now)
            .await?;
        Ok(Landed::Connected)
    }

    /// The provider's answer, as the handle this daemon will seal.
    async fn read(
        &self,
        provider: Provider,
        exchanged: &Exchanged,
        location: Option<&str>,
        now: UnixMillis,
    ) -> Result<Grant> {
        let body: Value = serde_json::from_str(exchanged.body())
            .map_err(|_unreadable| error::exchange_unreadable())?;
        let connected_at = self.grants.stamp(now);

        // Total over the providers, so a sixth cannot land with an exchange
        // that works and a handle nobody wrote.
        let grant = match provider {
            Provider::Slack => parse::slack(&body),
            Provider::Zoho => {
                let base = zoho::accounts_base(location);
                let mut extras = Map::new();
                extras.insert(HANDLE_ACCOUNTS_BASE.into(), base.into());
                let label = text(&body, WIRE_API_DOMAIN).unwrap_or_else(|| base.to_owned());
                parse::refresh_triple(provider, &body, &label, connected_at, extras)
            }
            Provider::Jira => return self.read_jira(&body, connected_at).await,
            Provider::Linear => parse::refresh_triple(
                provider,
                &body,
                provider.display_name(),
                connected_at,
                Map::new(),
            ),
            // A GitHub App's user-authorization answer is a bearer with no
            // refresh half and no install behind it yet, so it takes the Slack
            // shape's place rather than the triple's. It is not reachable
            // today — the App-install completion is its own callback — and it
            // is an arm rather than an absence so that stays a statement
            // somebody reads rather than a gap they discover.
            Provider::GitHub => None,
        };

        grant.ok_or_else(error::exchange_unreadable)
    }

    /// Jira's grant, with the site it is scoped to resolved first.
    async fn read_jira(&self, body: &Value, connected_at: UnixMillis) -> Result<Grant> {
        let access_token = text(body, "access_token").ok_or_else(error::exchange_unreadable)?;
        let site = jira::resolve(&self.client, self.jira_endpoint(), &access_token).await?;

        let mut extras = Map::new();
        extras.insert(jira::HANDLE_CLOUD_ID.into(), site.cloud_id.into());
        extras.insert(jira::HANDLE_SITE_URL.into(), site.url.into());

        parse::refresh_triple(Provider::Jira, body, &site.name, connected_at, extras)
            .ok_or_else(error::exchange_unreadable)
    }
}

/// Where this provider's code is redeemable.
///
/// Per-callback rather than per-provider for exactly one connector: a Zoho code
/// is only redeemable at the data centre that issued it, named by the
/// callback's own `location`. Everything else answers its registry endpoint.
fn token_endpoint(provider: Provider, location: Option<&str>) -> String {
    match provider {
        Provider::Zoho => zoho::token_endpoint(location),
        Provider::Slack | Provider::Jira | Provider::Linear => match provider.archetype() {
            Archetype::Oauth2(flow) => flow.token_endpoint.to_owned(),
            Archetype::AppInstall(install) => install.token_endpoint.to_owned(),
        },
        Provider::GitHub => match provider.archetype() {
            Archetype::AppInstall(install) => install.token_endpoint.to_owned(),
            Archetype::Oauth2(flow) => flow.token_endpoint.to_owned(),
        },
    }
}

/// One non-empty string field of a document.
fn text(document: &Value, name: &str) -> Option<String> {
    let value = document.get(name)?.as_str()?;
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::token_endpoint;
    use crate::provider::Provider;

    /// Zoho's endpoint follows the callback's data centre; nobody else's does.
    ///
    /// The load-bearing half is the second assertion: a code issued in Europe
    /// and redeemed at the US accounts server fails `invalid_grant`, and the
    /// person sees a connect that simply does not work in one region.
    #[test]
    fn only_zoho_redeems_at_a_per_callback_endpoint() {
        assert_eq!(
            token_endpoint(Provider::Zoho, Some("eu")),
            "https://accounts.zoho.eu/oauth/v2/token",
        );
        assert_eq!(
            token_endpoint(Provider::Zoho, None),
            "https://accounts.zoho.com/oauth/v2/token",
        );

        for provider in [Provider::Slack, Provider::Jira, Provider::Linear] {
            assert_eq!(
                token_endpoint(provider, Some("eu")),
                token_endpoint(provider, None),
                "`{provider}` has one token endpoint",
            );
        }
    }

    /// Every provider resolves to an endpoint that is a URL.
    #[test]
    fn every_provider_redeems_somewhere() {
        for provider in Provider::ALL.iter().copied() {
            assert!(
                token_endpoint(provider, None).starts_with("https://"),
                "`{provider}` redeems nowhere",
            );
        }
    }
}
