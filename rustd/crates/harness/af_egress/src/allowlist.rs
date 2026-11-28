//! The host an allowlist entry names: the one reading the sandbox's kernel set
//! and `http_request` both take, so a host one of them reaches is never a host
//! the other refuses.
//!
//! An entry is meant as a bare host, but neither grammar holds it to one: a
//! registry entry may add `:port` (`af_wire::runner` `registry_entry`), and a
//! fleet's `network.allow` refuses only whitespace (`af_fleet_runtime`
//! `config/raw/predicate.rs` `is_token`). So an entry is read as a URL, under
//! `https://` when it names no scheme, and only its host is kept. The
//! supervisor resolves that host into the addresses its sandbox reaches
//! (`afr_supervisor` `egress.rs`); the egress policy compares it to the host of the URL
//! a tool asks for (`policy/mod.rs`).

use url::{Host, Url};

/// What an entry naming its scheme carries; one without is read as a host.
const SCHEME_SEPARATOR: &str = "://";
/// The scheme a bare entry is read under, to find its host.
const ASSUMED_SCHEME: &str = "https://";

/// The host `entry` names, or `None` when no host can be read from it.
///
/// A port, scheme, path or userinfo is dropped. A domain is lowercased, as DNS
/// compares it, and an address literal is spelled without brackets, so
/// `https://API.Stripe.com:443/v1` names `api.stripe.com` and `[::1]:80` names
/// `::1`.
#[must_use]
pub fn allowlist_host(entry: &str) -> Option<String> {
    let url = if entry.contains(SCHEME_SEPARATOR) {
        Url::parse(entry)
    } else {
        Url::parse(&format!("{ASSUMED_SCHEME}{entry}"))
    };
    canonical_url(url.ok()?)
        .ok()?
        .host()
        .map(|host| spelled(&host))
}

/// Normalize the DNS root label once, before checking which network and
/// credential policies apply. The outgoing request uses the same host.
pub(crate) fn canonical_url(mut url: Url) -> Result<Url, url::ParseError> {
    if let Some(domain) = url.domain().and_then(|host| host.strip_suffix('.')) {
        if domain.is_empty() || domain.ends_with('.') {
            return Err(url::ParseError::InvalidDomainCharacter);
        }
        let host = domain.to_owned();
        url.set_host(Some(&host))?;
    }
    Ok(url)
}

/// Match an already canonical URL host against a configured bare hostname.
pub(crate) fn names_host(configured: &str, canonical: &str) -> bool {
    configured
        .strip_suffix('.')
        .unwrap_or(configured)
        .eq_ignore_ascii_case(canonical)
}

/// `host` as [`allowlist_host`] spells an entry's.
pub(crate) fn spelled(host: &Host<&str>) -> String {
    match host {
        Host::Domain(domain) => domain.to_ascii_lowercase(),
        Host::Ipv4(address) => address.to_string(),
        Host::Ipv6(address) => address.to_string(),
    }
}
