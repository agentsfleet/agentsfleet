//! The three dashboard URLs one connect round-trip travels through, and the
//! dashboard base every one of them, the invite link and the login link hang off.
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
//! The path is named once, in `RELAY_PATH` and `RELAY_LEAF`, and both are
//! derived from it, so they cannot drift.
//!
//! # The encoder is `url`'s, for the reason [`crate::oauth`]'s is
//!
//! There is no hand-written percent-encoder and no scan of the buffer to decide
//! between `?` and `&`: [`url::Url::query_pairs_mut`] is the same encoder the
//! authorize URL is composed through, and it cannot emit a `&` that splits a
//! parameter.

use afd_core::id::Uuid7;
use url::Url;

use crate::provider::Provider;

/// Where the dashboard mounts its connector relay, ONE SEGMENT PER ENTRY.
///
/// The one site that spells it (RULE UFS) — see the module note on why the
/// redirect URI and the relay must be one string.
///
/// Segments rather than the single `"api/connectors"` this held until the
/// relay was proven against a live deployment: `path_segments_mut` percent-
/// encodes every item it is handed, so a slash INSIDE one is not a separator
/// but data, and the minted redirect URI came out as
/// `/api%2Fconnectors/github/callback` — a path the dashboard does not mount
/// and a vendor refuses with `redirect_uri_mismatch`.
const RELAY_PATH: [&str; 2] = ["api", "connectors"];

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

/// The schemes a browser follows a dashboard link over.
const SCHEMES: [&str; 2] = ["http", "https"];

/// The dashboard's base URL, checked once at boot, and the pages under it.
///
/// Parsed where the deployment is configured rather than per request, so a
/// base that is not a URL refuses boot instead of failing each connect and
/// each invite on its own. Every page here is built by path segment through
/// `path_segments_mut`, so a base carrying a trailing slash, a sub-path, or a
/// port produces one well-formed URL — where `{s}{s}` concatenation gives
/// `https://host//api/...` for the first and silently drops the sub-path for
/// the second. The command-line login link is the one exception: `afd_tenant`
/// builds it as text from [`Dashboard::as_str`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dashboard(Url);

impl Dashboard {
    /// `raw` as a dashboard base, or `None` when no page can hang off it.
    ///
    /// Refuses anything but an absolute `http` or `https` URL, and one carrying
    /// a username, a password, a query or a fragment: each page appends path
    /// segments, and those would sit after the path or leak into every link.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Url::parse(raw)
            .ok()
            .filter(|base| SCHEMES.contains(&base.scheme()) && Self::is_bare(base))
            .map(Self)
    }

    /// Whether `base` carries nothing but a scheme, a host, a port and a path.
    fn is_bare(base: &Url) -> bool {
        base.username().is_empty()
            && base.password().is_none()
            && base.query().is_none()
            && base.fragment().is_none()
    }

    /// The base itself, for the one surface that composes its link from text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The page at `segments` under the base, each one encoded as one segment.
    #[must_use]
    pub fn page<'s>(&self, segments: impl IntoIterator<Item = &'s str>) -> Url {
        let mut page = self.0.clone();
        // `parse` admits only http(s), and a URL of either scheme always has a
        // path to extend, so the segments always land.
        if let Ok(mut path) = page.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        page
    }
}

/// The `redirect_uri` this deployment mints authorization codes against.
#[must_use]
pub fn relay_uri(dashboard: &Dashboard, provider: Provider) -> String {
    relay(dashboard, provider).into()
}

/// Where the browser goes when the provider hands the code back.
///
/// The same URL [`relay_uri`] answers, carrying what the provider sent. Absent
/// parameters are OMITTED rather than sent empty: `location=` is a data centre
/// named as the empty string, and the exchange would then redeem at the wrong
/// accounts server for a provider that has several.
#[must_use]
pub fn relay_url(dashboard: &Dashboard, provider: Provider, handoff: Handoff<'_>) -> String {
    let mut url = relay(dashboard, provider);
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
    url.into()
}

/// Where a person lands once the connect has finished.
#[must_use]
pub fn connected_url(dashboard: &Dashboard, workspace: &Uuid7) -> String {
    dashboard
        .page([INTEGRATIONS_PATH, workspace.as_str(), INTEGRATIONS_LEAF])
        .into()
}

/// The relay path under `dashboard`, with no query on it yet.
fn relay(dashboard: &Dashboard, provider: Provider) -> Url {
    dashboard.page(RELAY_PATH.into_iter().chain([provider.id(), RELAY_LEAF]))
}

#[cfg(test)]
#[path = "callback/tests.rs"]
mod tests;
