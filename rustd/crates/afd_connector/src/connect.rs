//! The connect flow, in the order that makes it safe.
//!
//! ```text
//!   start    mint a nonce, remember it, sign a state, build the authorize URL
//!   verify   signature, window, and WHO is presenting it — no store touched
//!   spend    the nonce, once, after the caller has re-authorised the workspace
//!   finish   redeem the code and land the grant   (see `crate::complete`)
//! ```
//!
//! # The ordering is enforced by the types, not by a comment
//!
//! [`Spent`] is constructible only by [`Connectors::spend`], and
//! [`Connectors::finish`] takes one. So a caller cannot redeem a code for a
//! state it has not spent, and it cannot spend one it has not verified —
//! [`Connectors::spend`] takes a [`Verified`], which only [`Connectors::verify`]
//! produces. `callback.zig` reaches the same ordering by writing the four steps
//! in one function and trusting nobody reorders them.
//!
//! # Why the workspace check sits BETWEEN verify and spend
//!
//! Consuming a nonce is destructive: whoever consumes it ends that connect
//! round-trip for everybody. If the spend came first, any authenticated person
//! who obtained a state — from a browser log, a referrer header, a shared
//! screen — could burn the starter's in-flight connect without being able to
//! complete it themselves. So the identity and workspace checks come first and
//! the spend is the last thing before the vendor call. `state.zig` states the
//! same ordering rule in its own header.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::SecretBytes;
use afd_redis::Redis;

use crate::app::PlatformApp;
use crate::error::Result;
use crate::exchange::Exchange;
use crate::grant::Grants;
use crate::provider::Provider;
use crate::registry::Archetype;
use crate::state::{self, Rejected, Verified, nonce};
use crate::{jira, oauth};

/// Where a person is sent to consent, or why they cannot be.
#[derive(Debug, Clone)]
pub enum Started {
    /// The provider's consent screen, with this round-trip's state on it.
    Consent(String),
    /// This deployment has configured no app for the provider.
    ///
    /// A value rather than an error, because nothing failed: an operator has
    /// not set the connector up, and the caller renders `UZ-CONN-001` for it.
    NotConfigured,
}

/// What one connect is being started for.
///
/// A struct rather than six positional arguments, and the reason is the pair a
/// caller could silently swap: `subject` and `redirect_uri` are both `&str`,
/// and transposing them mints a state bound to a URL and sends the person to a
/// callback named after their identity. Named fields make that a compile error
/// (`dispatch/write_rust.md`, RULE FN-RS).
#[derive(Debug, Clone, Copy)]
pub struct Starting<'s> {
    /// The workspace holding this deployment's own platform credentials.
    pub admin: &'s Uuid7,
    /// The workspace being connected.
    pub workspace: &'s Uuid7,
    /// What is being connected to.
    pub provider: Provider,
    /// Who pressed Connect, as the identity provider names them.
    pub subject: &'s str,
    /// Where the provider sends the browser back.
    pub redirect_uri: &'s str,
    /// What this deployment signs install states with.
    pub secret: &'s SecretBytes,
}

/// A state that has been verified, checked against its starter, and SPENT.
///
/// The receipt [`Connectors::finish`] requires — see the module note on why the
/// ordering is a type rather than a convention. Carries the workspace it was
/// minted for, parsed, so the completion cannot be pointed at another one.
#[derive(Debug, Clone)]
pub struct Spent {
    /// The workspace this connect lands in.
    workspace: Uuid7,
}

impl Spent {
    /// The workspace this connect lands in.
    #[must_use]
    pub const fn workspace(&self) -> &Uuid7 {
        &self.workspace
    }
}

/// Everything a connect round-trip acts through.
///
/// Cheap to clone: every field is a handle over a shared pool, a shared key, or
/// a shared client.
#[derive(Debug, Clone)]
pub struct Connectors {
    /// This deployment's own app credentials, and Slack's signing secret.
    pub(crate) app: PlatformApp,
    /// Where a completed connect lands.
    pub(crate) grants: Grants,
    /// The vendor conversation.
    pub(crate) exchange: Exchange,
    /// The client Jira's second call goes out on.
    ///
    /// The same client [`Self::exchange`] holds, kept here too because that one
    /// is a private field of a type whose whole surface is the token exchange —
    /// and a `client()` accessor on it would be an invitation to make some
    /// other call through the exchange.
    pub(crate) client: reqwest::Client,
    /// Where a round-trip's single-use slot lives.
    pub(crate) queue: Redis,
    /// Where a nonce is drawn from.
    pub(crate) entropy: Entropy,
    /// Where Jira's site listing is read, when a lane pointed it somewhere.
    pub(crate) jira_endpoint_override: Option<String>,
}

impl Connectors {
    /// Binds the flow to its stores, its vendor client and its entropy.
    #[must_use]
    pub const fn new(
        app: PlatformApp,
        grants: Grants,
        exchange: Exchange,
        client: reqwest::Client,
        queue: Redis,
        entropy: Entropy,
    ) -> Self {
        Self {
            app,
            grants,
            exchange,
            client,
            queue,
            entropy,
            jira_endpoint_override: None,
        }
    }

    /// The same flow, with Jira's site listing pointed at one endpoint.
    #[must_use]
    pub fn with_jira_endpoint(self, endpoint: String) -> Self {
        Self {
            jira_endpoint_override: Some(endpoint),
            ..self
        }
    }

    /// Where Jira's site listing is read for this deployment.
    pub(crate) fn jira_endpoint(&self) -> &str {
        self.jira_endpoint_override
            .as_deref()
            .unwrap_or(jira::ACCESSIBLE_RESOURCES)
    }

    /// Starts a connect: a remembered nonce, a signed state, a consent URL.
    ///
    /// The nonce is remembered BEFORE the URL is answered. A state whose slot
    /// was never written verifies at the callback and then fails to spend,
    /// which reads to the person as a forged callback for a connect they
    /// watched themselves start.
    ///
    /// # Errors
    /// Reports a host short of entropy, a store that would not remember the
    /// nonce, and a vault that would not answer for this deployment's app.
    pub async fn start(&self, starting: Starting<'_>, now: UnixMillis) -> Result<Started> {
        let Starting {
            admin,
            workspace,
            provider,
            subject,
            redirect_uri,
            secret,
        } = starting;
        let Some(credentials) = self.app.credentials(admin, provider).await? else {
            return Ok(Started::NotConfigured);
        };

        let binding = provider.state_binding();
        let nonce = nonce::mint(&self.entropy)?;
        let state = state::sign(binding, secret, workspace.as_str(), subject, &nonce, now);
        nonce::remember(&self.queue, binding, &nonce).await?;

        // Both archetypes start the same way — a consent screen carrying a
        // signed state — and differ only in what the code is redeemed FOR. The
        // match is here rather than a field on the flow because a third
        // archetype must fail to compile until somebody says where it sends a
        // person.
        let flow = match provider.archetype() {
            Archetype::Oauth2(flow) => flow,
            Archetype::AppInstall(install) => crate::registry::Oauth2Flow {
                authorize_endpoint: install.authorize_endpoint,
                token_endpoint: install.token_endpoint,
                // A GitHub App carries its own permissions rather than OAuth
                // scopes, so there is no list and nothing to join. The
                // delimiter is the standard's, which is what a reader would
                // assume anyway and what nothing here will ever spend.
                scopes: "",
                scope_delimiter: ' ',
                extra_query: &[],
                refresh: false,
            },
        };

        let Some(url) = oauth::authorize_url(flow, &credentials.client_id, redirect_uri, &state)
        else {
            // A shipped connector's endpoint is a URL — `crate::registry`'s
            // suite pins every one — so this is a registry edit that broke one,
            // and refusing the connect beats sending a person to nowhere.
            tracing::error!(
                provider = provider.id(),
                event = "connector_authorize_endpoint_unusable",
            );
            return Ok(Started::NotConfigured);
        };

        tracing::debug!(
            workspace_id = workspace.as_str(),
            provider = provider.id(),
            event = "connector_connect_initiated",
        );
        Ok(Started::Consent(url))
    }

    /// Verifies a presented state and that this is the person who started it.
    ///
    /// Touches no store, so a state that is not going to be acted on costs
    /// nothing but a hash — which is what keeps a replay attempt from being a
    /// way to make this daemon do work.
    ///
    /// # Errors
    /// [`Rejected`], which the caller logs by [`Rejected::reason`] and answers
    /// as one code — see [`afd_core::error_code::CONNECTOR_STATE_INVALID`].
    pub fn verify(
        &self,
        provider: Provider,
        secret: &SecretBytes,
        presented: &str,
        subject: &str,
        now: UnixMillis,
    ) -> core::result::Result<Verified, Rejected> {
        let binding = provider.state_binding();
        let verified = state::verify(binding, secret, presented, now)?;
        if verified.subject_matches(binding, secret, subject) {
            Ok(verified)
        } else {
            Err(Rejected::ForeignSubject)
        }
    }

    /// Spends this round-trip's single-use slot, once.
    ///
    /// `Ok(None)` for a slot already spent or expired, which the caller answers
    /// exactly as it answers a forged state: both mean start the connect again.
    ///
    /// # Errors
    /// Reports a store that would not answer, and a workspace the state names
    /// that is not a canonical identifier — the latter is a state this
    /// deployment SIGNED carrying an unparseable id, so it is this daemon's
    /// fault rather than the caller's.
    pub async fn spend(&self, provider: Provider, verified: &Verified) -> Result<Option<Spent>> {
        let workspace = Uuid7::parse(verified.workspace())?;
        let binding = provider.state_binding();
        Ok(nonce::consume(&self.queue, binding, verified.nonce())
            .await?
            .then_some(Spent { workspace }))
    }
}
