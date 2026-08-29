//! Where a person is sent to consent, and what is posted back to redeem it.
//!
//! Pure: composing a URL and composing a form, with no store and no client in
//! reach. The call that spends the form is [`crate::exchange`], which is what
//! keeps the two provably testable apart — a URL builder that needed a network
//! to exercise is one that gets exercised in production.
//!
//! # The encoder is `url`'s, not this crate's
//!
//! `oauth2.zig` hand-writes an RFC 3986 percent-encoder and calls it at four
//! sites, and `callback.zig` carries a SECOND copy of the same loop for its
//! relay. Neither exists here. RFC 6749 specifies the authorization
//! request's parameters as `application/x-www-form-urlencoded` in the query,
//! which is exactly what [`url::Url::query_pairs_mut`] writes, and the exchange
//! body is the same encoding again. One encoder, and it is not ours to get
//! wrong — the crate audit's finding applied one surface over.

use url::Url;

use crate::registry::Oauth2Flow;

/// The grant this daemon asks for, which is the only one it knows how to hold.
const GRANT_AUTHORIZATION_CODE: &str = "authorization_code";

/// The `response_type` an authorization-code flow asks for.
const RESPONSE_TYPE_CODE: &str = "code";

/// Parameter names, named once each rather than spelled at both sites.
const PARAM_RESPONSE_TYPE: &str = "response_type";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_CLIENT_ID: &str = "client_id";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_CLIENT_SECRET: &str = "client_secret";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_SCOPE: &str = "scope";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_REDIRECT_URI: &str = "redirect_uri";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_STATE: &str = "state";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_GRANT_TYPE: &str = "grant_type";
/// See [`PARAM_RESPONSE_TYPE`].
const PARAM_CODE: &str = "code";

/// Where the browser is sent to consent.
///
/// `None` for a flow whose authorize endpoint is not a URL — which cannot
/// happen for a shipped connector, because [`crate::registry`]'s suite pins
/// every endpoint as `https://`, and is an `Option` rather than a panic so a
/// registry edit that broke one refuses the connect instead of the process.
#[must_use]
pub fn authorize_url(
    flow: Oauth2Flow,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
) -> Option<String> {
    let mut url = Url::parse(flow.authorize_endpoint).ok()?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair(PARAM_RESPONSE_TYPE, RESPONSE_TYPE_CODE);
        query.append_pair(PARAM_CLIENT_ID, client_id);
        // Omitted rather than sent empty for the one archetype that has none:
        // a GitHub App carries its own permissions, and `scope=` is a request
        // for nothing rather than a request for the default.
        if !flow.scopes.is_empty() {
            query.append_pair(PARAM_SCOPE, flow.scopes);
        }
        query.append_pair(PARAM_REDIRECT_URI, redirect_uri);
        query.append_pair(PARAM_STATE, state);
        for (name, value) in flow.extra_query {
            query.append_pair(name, value);
        }
    }
    Some(url.into())
}

/// What is posted to redeem an authorization code.
///
/// Answered as pairs rather than as an encoded string, so the encoding happens
/// once — in the client that sends it — instead of here and again there.
///
/// The client secret is in this list, which is why nothing that handles the
/// return value may log it. [`crate::exchange`] is the only caller and says so.
#[must_use]
pub fn exchange_form<'f>(
    client_id: &'f str,
    client_secret: &'f str,
    code: &'f str,
    redirect_uri: &'f str,
) -> [(&'static str, &'f str); 5] {
    [
        (PARAM_GRANT_TYPE, GRANT_AUTHORIZATION_CODE),
        (PARAM_CLIENT_ID, client_id),
        (PARAM_CLIENT_SECRET, client_secret),
        (PARAM_CODE, code),
        (PARAM_REDIRECT_URI, redirect_uri),
    ]
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::panic,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{authorize_url, exchange_form};
    use crate::provider::Provider;
    use crate::registry::Archetype;

    /// The callback this deployment publishes, as a connect would build it.
    const REDIRECT: &str = "https://app.example.test/api/connectors/slack/callback";

    /// The flow one provider runs, for a case that needs a real one.
    fn flow(provider: Provider) -> crate::registry::Oauth2Flow {
        match provider.archetype() {
            Archetype::Oauth2(flow) => flow,
            Archetype::AppInstall(_) => panic!("`{provider}` is not an OAuth connector"),
        }
    }

    /// Every shared parameter lands, and the redirect URI is escaped.
    ///
    /// The escape is the load-bearing half: an unescaped `://` and `/` would
    /// truncate the query at the provider and send the person back to a URL
    /// this deployment never published.
    #[test]
    fn the_authorize_url_carries_the_five_shared_parameters_escaped() {
        let url = authorize_url(flow(Provider::Slack), "CID123", REDIRECT, "st.mac")
            .expect("a shipped connector's endpoint is a URL");

        assert!(url.starts_with("https://slack.com/oauth/v2/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=CID123"));
        assert!(url.contains("scope=app_mentions%3Aread%2Cchat%3Awrite"));
        assert!(url.contains("state=st.mac"));
        assert!(url.contains(
            "redirect_uri=https%3A%2F%2Fapp.example.test%2Fapi%2Fconnectors%2Fslack%2Fcallback",
        ));
    }

    /// A provider's extra parameters ride the same encoder as the shared ones.
    ///
    /// Atlassian refuses an authorize request with no `audience`, so this is
    /// the parameter whose absence would look like a working connector that
    /// nobody can connect.
    #[test]
    fn a_providers_own_parameters_are_appended_and_escaped() {
        let url = authorize_url(flow(Provider::Jira), "CID", REDIRECT, "st")
            .expect("a shipped connector's endpoint is a URL");

        assert!(url.contains("audience=api.atlassian.com"));
        assert!(url.contains("prompt=consent"));
        // Space-delimited scopes survive as an encoded space rather than as a
        // raw one, which would end the query parameter at the first gap.
        assert!(!url.contains("read:jira-work read"));
    }

    /// A state carrying reserved characters survives the round trip.
    ///
    /// The state is base64url and hex by construction, so this is defence
    /// rather than a live case — but it is the parameter an attacker controls
    /// the least and the encoder is what keeps that true.
    #[test]
    fn a_state_with_reserved_characters_does_not_escape_its_parameter() {
        let url = authorize_url(flow(Provider::Slack), "CID", REDIRECT, "a&b=c")
            .expect("a shipped connector's endpoint is a URL");

        assert!(url.contains("state=a%26b%3Dc"));
        assert!(!url.contains("state=a&b=c"));
    }

    /// The exchange asks for the authorization-code grant and names all five.
    #[test]
    fn the_exchange_form_carries_the_grant_and_its_four_values() {
        let form = exchange_form("cid", "sec", "the-code", REDIRECT);

        assert_eq!(form[0], ("grant_type", "authorization_code"));
        assert_eq!(form[1].1, "cid");
        assert_eq!(form[2].1, "sec");
        assert_eq!(form[3].1, "the-code");
        assert_eq!(form[4].1, REDIRECT);
    }
}
