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

/// What separates userinfo from the host it is disguising.
const USERINFO_SEPARATOR: char = '@';

/// Rejected because browsers normalise it to [`PATH_SEPARATOR`] and this does
/// not, which is a disagreement an attacker chooses the side of.
const BACKSLASH: char = '\\';

/// `vendor`'s path on the host a lane pinned, or `vendor` when none did.
///
/// `None` means a pin WAS supplied and is not a usable origin, and the caller
/// must refuse rather than call. Falling back to the vendor there would be the
/// opposite of what the knob is for: a lane sets it to keep a test off the real
/// vendor, so a typo in it would send a freshly minted bearer token to
/// Atlassian from CI — the one outcome pinning exists to prevent.
///
/// The PATH is the vendor's and is never replaced. A lane pins where requests
/// land, not what they ask for, so a fake provider serving the real paths is
/// answering the questions the daemon actually asks.
pub(crate) fn redirected(vendor: &str, pinned: Option<&str>) -> Option<String> {
    let Some(pin) = pinned else {
        return Some(vendor.to_owned());
    };
    let origin = origin_of(pin)?;
    Some(match path_of(vendor) {
        Some(path) => format!("{origin}{path}"),
        None => origin.to_owned(),
    })
}

/// The `scheme://host[:port]` of an absolute URL, if it has one.
///
/// Deliberately strict about the authority rather than permissive. An empty
/// host parses as a URL to almost nothing — `http:///a` is scheme `http` and
/// path `/a` — and composing on it yields a request whose destination the
/// resolver decides. Userinfo is worse: `http://vendor.example@evil.test`
/// reads as the vendor to a person and resolves to `evil.test`, which is the
/// oldest trick there is for making a hostile host look like a friendly one.
/// A backslash is rejected for the same reason browsers normalise it to `/`.
fn origin_of(url: &str) -> Option<&str> {
    let (scheme, authority) = url.split_once(SCHEME_SEPARATOR)?;
    if scheme.is_empty() {
        return None;
    }
    let host = authority.split(PATH_SEPARATOR).next()?;
    if host.is_empty() || host.contains(USERINFO_SEPARATOR) || host.contains(BACKSLASH) {
        return None;
    }
    url.get(..scheme.len() + SCHEME_SEPARATOR.len() + host.len())
}

/// Everything from the path onward — the part a pinned host does not replace.
fn path_of(url: &str) -> Option<&str> {
    url.get(origin_of(url)?.len()..)
}

#[cfg(test)]
mod tests {
    use super::{origin_of, redirected};

    /// The vendor's own endpoint, spelled exactly as declared.
    #[test]
    fn nothing_pinned_reaches_the_vendor() {
        let vendor = "https://api.atlassian.com/oauth/token/accessible-resources";
        assert_eq!(redirected(vendor, None).as_deref(), Some(vendor));
    }

    /// A pinned lane moves the HOST and keeps the vendor's path.
    ///
    /// The path half is load-bearing: a fake provider only proves something if
    /// the daemon asks it the question it asks Atlassian.
    #[test]
    fn a_pinned_lane_moves_the_host_and_keeps_the_path() {
        assert_eq!(
            redirected(
                "https://api.atlassian.com/oauth/token/accessible-resources",
                Some("http://127.0.0.1:9931/oauth/v2/token"),
            )
            .as_deref(),
            Some("http://127.0.0.1:9931/oauth/token/accessible-resources"),
        );
    }

    /// Only the origin of the pin is used — its own path is discarded.
    ///
    /// The knob is the token endpoint, so it arrives carrying the exchange's
    /// path. Keeping it would dial the token route asking for a site listing.
    #[test]
    fn only_the_origin_of_the_pin_is_used() {
        for pinned in [
            "http://127.0.0.1:9931",
            "http://127.0.0.1:9931/",
            "http://127.0.0.1:9931/oauth/v2/token",
        ] {
            assert_eq!(
                redirected("https://vendor.example/a/b", Some(pinned)).as_deref(),
                Some("http://127.0.0.1:9931/a/b"),
                "`{pinned}`",
            );
        }
    }

    /// An unusable pin REFUSES rather than falling back to the vendor.
    ///
    /// The case this suite got wrong the first time, and the reason it matters:
    /// a lane pins to keep a test off the real vendor, so answering the vendor
    /// on a typo sends a freshly minted bearer token to Atlassian from CI.
    /// Every entry below is a pin somebody could plausibly write.
    #[test]
    fn a_pin_that_is_not_a_usable_origin_refuses() {
        for pinned in [
            "",
            "127.0.0.1:9931",
            "://nohost",
            "http://",
            // The one the hand-rolled parser accepted: an empty host, which
            // composes `http:///a/b` and lets the resolver pick a destination.
            "http:///127.0.0.1:9931/token",
            // Userinfo — reads as the vendor, resolves to the other host.
            "http://vendor.example@evil.test/token",
            "http:\\\\127.0.0.1:9931",
        ] {
            assert_eq!(
                redirected("https://vendor.example/a/b", Some(pinned)),
                None,
                "`{pinned}` must refuse, never fall back to the vendor",
            );
        }
    }

    /// A vendor endpoint with no path resolves to the pinned origin.
    #[test]
    fn a_vendor_endpoint_with_no_path_becomes_the_pinned_origin() {
        assert_eq!(
            redirected("https://vendor.example", Some("http://127.0.0.1:9931/t")).as_deref(),
            Some("http://127.0.0.1:9931"),
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
