//! The two dashboard URLs one connect round-trip travels through, and the
//! dashboard base every one of them, the invite link and the login link hang off.
//!
//! [`relay_uri`] is the `redirect_uri` a provider mints its authorization code
//! against. The provider sends the browser there, and the exchange echoes the
//! same string back: the vendor compares the two, and a mismatch fails as
//! `redirect_uri_mismatch`, which reads to an operator like a rotated client
//! secret. The path is named once, in `RELAY_PATH` and `RELAY_LEAF`.

use afd_core::id::Uuid7;
use url::Url;

use crate::provider::Provider;

/// Where the dashboard mounts its connector relay, ONE SEGMENT PER ENTRY.
///
/// The one site that spells it (RULE UFS) — see the module note on why the
/// redirect URI must not drift.
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
    dashboard
        .page(RELAY_PATH.into_iter().chain([provider.id(), RELAY_LEAF]))
        .into()
}

/// Where a person lands once the connect has finished.
#[must_use]
pub fn connected_url(dashboard: &Dashboard, workspace: &Uuid7) -> String {
    dashboard
        .page([INTEGRATIONS_PATH, workspace.as_str(), INTEGRATIONS_LEAF])
        .into()
}

#[cfg(test)]
mod tests;
