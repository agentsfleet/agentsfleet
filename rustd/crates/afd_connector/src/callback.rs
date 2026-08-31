//! The three dashboard URLs one connect round-trip travels through.
//!
//! # Why they are one module and not three call sites
//!
//! [`relay_uri`] is the `redirect_uri` a provider mints its authorization code
//! against, and [`relay_url`] is where the browser is actually sent when that
//! provider hands the code back. They must be byte-identical up to the query,
//! because the exchange echoes the redirect URI and the provider compares it to
//! the one the code was minted for — a mismatch fails as `redirect_uri_mismatch`
//! at the vendor and reads to an operator like a rotated client secret.
//!
//! `connect.zig` builds the first with `CALLBACK_PATH_FMT` and `callback.zig`
//! the second with `CALLBACK_RELAY_PATH_FMT`, two format strings in two files
//! spelling one path. Here the path is named once and both are derived from it,
//! so they cannot drift.
//!
//! # The encoder is `url`'s, for the reason [`crate::oauth`]'s is
//!
//! `callback.zig` carries its own `percentEncode` loop and an
//! `appendRelayParam` that scans the buffer it is building to decide between
//! `?` and `&`. Neither exists here: [`url::Url::query_pairs_mut`] is the same
//! encoder the authorize URL is composed through, and it cannot emit a `&` that
//! splits a parameter.

use afd_core::id::Uuid7;
use url::Url;

use crate::provider::Provider;

/// Where the dashboard mounts its connector relay.
///
/// The one site that spells it (RULE UFS) — see the module note on why the
/// redirect URI and the relay must be one string.
const RELAY_PATH: &str = "api/connectors";

/// The trailing segment of the relay path — see [`RELAY_PATH`].
const RELAY_LEAF: &str = "callback";

/// Where the dashboard shows a workspace's connections.
const INTEGRATIONS_PATH: &str = "w";

/// See [`INTEGRATIONS_PATH`].
const INTEGRATIONS_LEAF: &str = "integrations";

/// Query parameters a provider hands back, named once each.
const PARAM_CODE: &str = "code";
/// See [`PARAM_CODE`].
const PARAM_STATE: &str = "state";
/// See [`PARAM_CODE`].
const PARAM_LOCATION: &str = "location";
/// See [`PARAM_CODE`].
const PARAM_INSTALLATION_ID: &str = "installation_id";

/// What a provider handed back, on its way to the dashboard.
///
/// A struct rather than four positional `Option<&str>` arguments, which is the
/// shape that matters most here: all four are optional strings, so a
/// transposition would compile and forward a data centre as an authorization
/// code — and the browser would land on a relay that then failed the exchange
/// with a vendor sentence nobody can act on.
#[derive(Debug, Clone, Copy, Default)]
pub struct Handoff<'h> {
    /// The authorization code, absent when the person declined consent.
    pub code: Option<&'h str>,
    /// This round-trip's signed state. The one parameter a relay requires.
    pub state: &'h str,
    /// Which data centre issued the code, for the one provider that has several.
    pub location: Option<&'h str>,
    /// The installation the person chose, for the App archetype.
    pub installation_id: Option<&'h str>,
}

/// The `redirect_uri` this deployment mints authorization codes against.
///
/// `None` for a dashboard base that is not a URL, which is a boot-time
/// misconfiguration rather than anything a person did: the connect refuses
/// rather than sending somebody to a page that cannot exist.
#[must_use]
pub fn relay_uri(dashboard: &str, provider: Provider) -> Option<String> {
    Some(relay(dashboard, provider)?.into())
}

/// Where the browser goes when the provider hands the code back.
///
/// The same URL [`relay_uri`] answers, carrying what the provider sent. Absent
/// parameters are OMITTED rather than sent empty: `location=` is a data centre
/// named as the empty string, and the exchange would then redeem at the wrong
/// accounts server for a provider that has several.
///
/// `None` as [`relay_uri`].
#[must_use]
pub fn relay_url(dashboard: &str, provider: Provider, handoff: Handoff<'_>) -> Option<String> {
    let mut url = relay(dashboard, provider)?;
    {
        let mut query = url.query_pairs_mut();
        if let Some(code) = handoff.code {
            query.append_pair(PARAM_CODE, code);
        }
        query.append_pair(PARAM_STATE, handoff.state);
        if let Some(location) = handoff.location {
            query.append_pair(PARAM_LOCATION, location);
        }
        if let Some(installation) = handoff.installation_id {
            query.append_pair(PARAM_INSTALLATION_ID, installation);
        }
    }
    Some(url.into())
}

/// Where a person lands once the connect has finished.
///
/// `None` as [`relay_uri`]. A connect that succeeded and cannot build this is
/// still a connect that succeeded — see the caller on why that is a 200 rather
/// than a failure.
#[must_use]
pub fn connected_url(dashboard: &str, workspace: &Uuid7) -> Option<String> {
    let mut url = Url::parse(dashboard).ok()?;
    url.path_segments_mut().ok()?.pop_if_empty().extend([
        INTEGRATIONS_PATH,
        workspace.as_str(),
        INTEGRATIONS_LEAF,
    ]);
    Some(url.into())
}

/// The relay path under `dashboard`, with no query on it yet.
///
/// Built through `path_segments_mut` rather than by formatting, so a base URL
/// carrying a trailing slash, a sub-path, or a port produces one well-formed
/// URL — where `{s}{s}` concatenation gives `https://host//api/...` for the
/// first and silently drops the sub-path for the second.
fn relay(dashboard: &str, provider: Provider) -> Option<Url> {
    let mut url = Url::parse(dashboard).ok()?;
    url.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .extend([RELAY_PATH, provider.id(), RELAY_LEAF]);
    Some(url)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{Handoff, connected_url, relay_uri, relay_url};
    use crate::provider::Provider;
    use afd_core::id::Uuid7;

    /// The dashboard base a suite builds URLs under.
    const DASHBOARD: &str = "https://app.example.test";

    /// A workspace identifier the destination is built for.
    const WORKSPACE: &str = "01920000-0000-7000-8000-000000000001";

    /// The relay a code is minted against is the relay it comes back to.
    ///
    /// The load-bearing property of this module: an exchange echoes the
    /// redirect URI, so a relay that differed from the minted one by a slash
    /// fails at the vendor with `redirect_uri_mismatch` — which reads like a
    /// rotated client secret and sends an operator to the wrong place.
    #[test]
    fn the_minted_redirect_uri_is_the_relay_the_browser_returns_to() {
        for provider in Provider::ALL.iter().copied() {
            let minted = relay_uri(DASHBOARD, provider).expect("a URL base");
            let returned = relay_url(
                DASHBOARD,
                provider,
                Handoff {
                    state: "s",
                    ..Handoff::default()
                },
            )
            .expect("a URL base");

            assert_eq!(
                returned.split('?').next(),
                Some(minted.as_str()),
                "`{provider}` must return to the relay its code was minted against",
            );
        }
    }

    /// A trailing slash on the configured base does not double.
    ///
    /// An operator writes `https://app.example.test/` as readily as without,
    /// and `{s}{s}` concatenation turns that into `//api/connectors/...` — a
    /// different path to the provider, and therefore a different redirect URI
    /// from the one the code was minted against.
    #[test]
    fn a_trailing_slash_on_the_base_does_not_become_a_double_slash() {
        assert_eq!(
            relay_uri("https://app.example.test/", Provider::Slack),
            relay_uri(DASHBOARD, Provider::Slack),
        );
    }

    /// An absent parameter is omitted, never sent empty.
    ///
    /// `location=` is the one that bites: Zoho redeems only at the data centre
    /// that issued the code, and an empty location reads as "unspecified" in
    /// one place and as a value in another.
    #[test]
    fn an_absent_parameter_is_omitted_rather_than_sent_empty() {
        let url = relay_url(
            DASHBOARD,
            Provider::Zoho,
            Handoff {
                code: Some("abc"),
                state: "signed",
                ..Handoff::default()
            },
        )
        .expect("a URL base");

        assert!(url.contains("code=abc"));
        assert!(url.contains("state=signed"));
        assert!(!url.contains("location"));
        assert!(!url.contains("installation_id"));
    }

    /// Every parameter a provider can send survives encoding.
    ///
    /// The `&` is the case the hand-rolled encoder this replaces got wrong: a
    /// code carrying one would otherwise split into two parameters and the
    /// exchange would redeem a truncated code.
    #[test]
    fn a_parameter_carrying_a_separator_does_not_split_into_two() {
        let url = relay_url(
            DASHBOARD,
            Provider::Slack,
            Handoff {
                code: Some("a&state=forged"),
                state: "real",
                ..Handoff::default()
            },
        )
        .expect("a URL base");

        assert!(url.contains("code=a%26state%3Dforged"));
        assert_eq!(url.matches("state=").count(), 1);
    }

    /// A base that is not a URL builds nothing, rather than half a URL.
    #[test]
    fn a_base_that_is_not_a_url_answers_nothing() {
        let workspace = Uuid7::parse(WORKSPACE).expect("a canonical identifier");

        for base in ["", "not a url", "/relative"] {
            assert_eq!(relay_uri(base, Provider::Jira), None, "`{base}` is no base");
            assert_eq!(connected_url(base, &workspace), None, "`{base}` is no base");
        }
    }

    /// The destination names the workspace the connect landed in.
    #[test]
    fn a_finished_connect_lands_on_its_own_workspaces_page() {
        let workspace = Uuid7::parse(WORKSPACE).expect("a canonical identifier");

        assert_eq!(
            connected_url(DASHBOARD, &workspace).as_deref(),
            Some("https://app.example.test/w/01920000-0000-7000-8000-000000000001/integrations"),
        );
    }
}
