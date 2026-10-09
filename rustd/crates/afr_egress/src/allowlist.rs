//! The host an allowlist entry names: the one reading the sandbox's kernel set
//! and `http_request` both take, so a host one of them reaches is never a host
//! the other refuses.
//!
//! An entry is meant as a bare host, but neither grammar holds it to one: a
//! registry entry may add `:port` (`afd_wire::runner` `registry_entry`), and a
//! fleet's `network.allow` refuses only whitespace (`afd_fleet_runtime`
//! `config/raw/predicate.rs` `is_token`). So an entry is read as a URL, under
//! `https://` when it names no scheme, and only its host is kept. The
//! supervisor resolves that host into the addresses its sandbox reaches
//! (`afr_supervisor` `egress.rs`); admission compares it to the host of the URL
//! a tool asks for (`admission.rs`).

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
    url.ok()?.host().map(|host| spelled(&host))
}

/// `host` as [`allowlist_host`] spells an entry's.
pub(crate) fn spelled(host: &Host<&str>) -> String {
    match host {
        Host::Domain(domain) => domain.to_ascii_lowercase(),
        Host::Ipv4(address) => address.to_string(),
        Host::Ipv6(address) => address.to_string(),
    }
}
