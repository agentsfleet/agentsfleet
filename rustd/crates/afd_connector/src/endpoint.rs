//! Where a connect's calls actually go, when a lane has pinned a host.
//!
//! # One knob, every host a connect dials
//!
//! A connect is not one request. The exchange redeems the code, and some
//! providers then dial a SECOND host before the grant is sealed — Jira reads
//! the site its token is scoped to, and `github/ownership.zig` proves the
//! person reaches the installation. A lane that redirected only the exchange
//! would leave that second call dialling the real vendor from a test: a pass
//! that proves nothing, and CI traffic arriving at Atlassian.
//!
//! So there is ONE knob — the endpoint [`crate::exchange::Exchange`] was
//! pointed at — and every other host is derived from its ORIGIN.
//! `jira/callback.zig:87` composes the same way from the same single override
//! on the other daemon; this is that composition, shared, so the next
//! connector with a second call gets it by calling [`redirected`] rather than
//! by remembering that the problem exists.

/// What separates a URL's scheme from its authority.
const SCHEME_SEPARATOR: &str = "://";

/// What begins the path, and so ends the origin.
const PATH_SEPARATOR: char = '/';

/// `vendor`'s path, on the host a lane pinned — or `vendor` itself.
///
/// The PATH is the vendor's and is never replaced: a lane pins where requests
/// land, not what they ask for, so a fake provider serving the real paths is
/// answering the questions the daemon actually asks rather than a shape the
/// test invented. A `pinned` value that is not an absolute URL leaves `vendor`
/// alone, because composing on a host that cannot be parsed would dial
/// somewhere neither the lane nor the vendor chose.
pub(crate) fn redirected(vendor: &str, pinned: Option<&str>) -> String {
    let Some(origin) = pinned.and_then(origin_of) else {
        return vendor.to_owned();
    };
    match path_of(vendor) {
        Some(path) => format!("{origin}{path}"),
        None => origin.to_owned(),
    }
}

/// The `scheme://host[:port]` of an absolute URL.
fn origin_of(url: &str) -> Option<&str> {
    let (scheme, authority) = url.split_once(SCHEME_SEPARATOR)?;
    if scheme.is_empty() || authority.is_empty() {
        return None;
    }
    let host = authority.split(PATH_SEPARATOR).next()?;
    url.get(..scheme.len() + SCHEME_SEPARATOR.len() + host.len())
}

/// Everything from the path onward — the part a pinned host does not replace.
fn path_of(url: &str) -> Option<&str> {
    url.get(origin_of(url)?.len()..)
}

#[cfg(test)]
mod tests {
    use super::{origin_of, redirected};

    /// An unpinned call reaches the vendor, spelled exactly as declared.
    #[test]
    fn nothing_pinned_leaves_the_vendors_endpoint_alone() {
        let vendor = "https://api.atlassian.com/oauth/token/accessible-resources";
        assert_eq!(redirected(vendor, None), vendor);
    }

    /// A pinned lane moves the HOST and keeps the vendor's path.
    ///
    /// The path half is the load-bearing one: a fake provider is only proving
    /// something if the daemon asks it the question it asks Atlassian. Swapping
    /// the path too would let a lane pass against a route the real vendor does
    /// not serve.
    #[test]
    fn a_pinned_lane_moves_the_host_and_keeps_the_path() {
        assert_eq!(
            redirected(
                "https://api.atlassian.com/oauth/token/accessible-resources",
                Some("http://127.0.0.1:9931/oauth/v2/token"),
            ),
            "http://127.0.0.1:9931/oauth/token/accessible-resources",
        );
    }

    /// The pin is read for its origin only — its own path is discarded.
    ///
    /// The knob is the token endpoint, so it arrives carrying the exchange's
    /// path. A composition that kept it would dial the token route asking for
    /// a site listing.
    #[test]
    fn only_the_origin_of_the_pin_is_used() {
        for pinned in [
            "http://127.0.0.1:9931",
            "http://127.0.0.1:9931/",
            "http://127.0.0.1:9931/oauth/v2/token",
        ] {
            assert_eq!(
                redirected("https://vendor.example/a/b", Some(pinned)),
                "http://127.0.0.1:9931/a/b",
                "`{pinned}`",
            );
        }
    }

    /// A pin that is not an absolute URL leaves the vendor's endpoint alone.
    ///
    /// The fail-safe direction: an unparseable pin means the lane is
    /// misconfigured, and dialling the vendor is the behaviour a reader can
    /// diagnose from the request that arrives there.
    #[test]
    fn a_pin_that_is_not_an_absolute_url_changes_nothing() {
        let vendor = "https://vendor.example/a/b";
        for pinned in ["", "127.0.0.1:9931", "://nohost", "http://"] {
            assert_eq!(redirected(vendor, Some(pinned)), vendor, "`{pinned}`");
        }
    }

    /// A vendor endpoint with no path resolves to the pinned origin.
    #[test]
    fn a_vendor_endpoint_with_no_path_becomes_the_pinned_origin() {
        assert_eq!(
            redirected("https://vendor.example", Some("http://127.0.0.1:9931/t")),
            "http://127.0.0.1:9931",
        );
    }

    /// The origin stops at the first path separator, and keeps the port.
    #[test]
    fn an_origin_is_the_scheme_the_host_and_the_port() {
        assert_eq!(
            origin_of("https://host.example:8443/a/b"),
            Some("https://host.example:8443"),
        );
        assert_eq!(
            origin_of("https://host.example"),
            Some("https://host.example")
        );
        assert_eq!(origin_of("host.example/a"), None);
    }
}
