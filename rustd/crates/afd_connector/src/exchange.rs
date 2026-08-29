//! Redeeming an authorization code at the provider that issued it.
//!
//! # The one call in this crate that leaves the process
//!
//! It is deliberately the whole of what this module does. Composing the form is
//! [`crate::oauth`]'s, reading the answer is [`crate::grant`]'s, and what is
//! left here is the send and the classification of what came back. That split
//! is what lets the other two be proven with no network in reach.
//!
//! # Nothing here logs a body
//!
//! The request carries this deployment's client secret and the answer carries a
//! tenant's freshly minted token. Neither is logged, at any level, and the
//! error this module raises carries a STATUS rather than a body for the same
//! reason (RULE VLT). What an operator gets is which provider, and what it
//! answered.

use afd_crypto::secret::SecretBytes;

use crate::error::{self, Result};
use crate::oauth;
use crate::provider::Provider;

/// What a provider answered the exchange with.
///
/// The raw body, because the shape is the PROVIDER's — Slack's
/// `{"ok":true,"access_token":…}` is not Linear's — and reading it is
/// [`crate::grant`]'s, beside the parse that knows which provider it holds.
#[derive(Clone)]
pub struct Exchanged {
    /// The body, unread. Never logged — see the module note.
    body: String,
}

impl Exchanged {
    /// The body, for the provider parse that knows its shape.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// Renders as its provenance rather than its content.
///
/// Derived `Debug` would put a bearer token in whatever printed it — a
/// `dbg!`, a panic message, a `?err` field on a trace line — so the derive is
/// deliberately not taken.
impl std::fmt::Debug for Exchanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Exchanged(<redacted provider grant>)")
    }
}

/// The platform app credentials one connector's exchange is signed with.
///
/// Read from the deployment's admin workspace — one OAuth app per connector
/// serving every tenant. The secret half is [`SecretBytes`], which zeroes when
/// it drops, so it is never a `String` on the way through.
#[derive(Debug)]
pub struct AppCredentials {
    /// The public half, which rides the authorize URL.
    pub client_id: String,
    /// The half that proves this deployment owns the app.
    pub client_secret: SecretBytes,
}

/// The client an exchange goes out on.
///
/// A struct rather than a free function taking a client, because the endpoint
/// override belongs beside it: an integration lane points every exchange at a
/// loopback provider, and threading that through each call site is how one call
/// site ends up dialling the real vendor from a test.
#[derive(Debug, Clone)]
pub struct Exchange {
    /// The client every call here goes out on. Cheap to clone — a handle over
    /// a shared connection pool.
    client: reqwest::Client,
    /// Where every exchange goes instead, when a lane pointed them somewhere.
    ///
    /// Always wins when set, including over Zoho's per-callback data centre:
    /// a lane that has pinned an endpoint must not have a `location` parameter
    /// send one provider's exchange to the real vendor.
    endpoint_override: Option<String>,
}

impl Exchange {
    /// Binds the exchange to a client, dialling providers as they publish.
    #[must_use]
    pub const fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            endpoint_override: None,
        }
    }

    /// The same exchange, with every call pointed at one endpoint.
    #[must_use]
    pub fn pointed_at(self, endpoint: String) -> Self {
        Self {
            endpoint_override: Some(endpoint),
            ..self
        }
    }

    /// Redeems `code` at `endpoint` for whatever grant the provider issues.
    ///
    /// # Errors
    /// Reports a provider that could not be reached and one that answered and
    /// refused — see [`crate::error`] on why those are two failures rather than
    /// one. A body this build cannot read is [`crate::grant`]'s to report.
    pub async fn redeem(
        &self,
        provider: Provider,
        endpoint: &str,
        credentials: &AppCredentials,
        code: &str,
        redirect_uri: &str,
    ) -> Result<Exchanged> {
        let secret = String::from_utf8_lossy(credentials.client_secret.expose()).into_owned();
        let form = oauth::exchange_form(&credentials.client_id, &secret, code, redirect_uri);
        let endpoint = self.endpoint_override.as_deref().unwrap_or(endpoint);

        let answer = self
            .client
            .post(endpoint)
            .form(&form)
            .send()
            .await
            .inspect_err(|_source| {
                // The provider and nothing else: the form held the client
                // secret, and `reqwest`'s own error renders the URL it dialled.
                tracing::warn!(
                    provider = provider.id(),
                    event = "connector_exchange_unreachable",
                );
            })?;

        let status = answer.status();
        if !status.is_success() {
            let refused = status.as_u16();
            tracing::warn!(
                provider = provider.id(),
                status = refused,
                event = "connector_exchange_refused",
            );
            return Err(error::exchange_refused(refused));
        }

        // Read as text rather than through `Response::json`: this workspace
        // resolves `reqwest` without its `json` feature, and turning one on for
        // one call site would put `serde_json` inside every other crate's HTTP
        // client too. `afd_cron::qstash` reads its own answer the same way.
        Ok(Exchanged {
            body: answer.text().await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Exchanged;

    /// The body never renders, however it is printed.
    ///
    /// A derived `Debug` would put a tenant's freshly minted bearer token into
    /// a panic message or a `?err` trace field, which is exactly the class of
    /// leak RULE VLT exists for.
    #[test]
    fn a_provider_grant_does_not_render_its_body() {
        let exchanged = Exchanged {
            body: String::from(r#"{"access_token":"xoxb-not-a-real-token"}"#),
        };

        let rendered = format!("{exchanged:?}");

        assert!(!rendered.contains("xoxb"));
        assert_eq!(rendered, "Exchanged(<redacted provider grant>)");
    }
}
