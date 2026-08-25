//! Whether a tenant-supplied endpoint may be dialed, and by which provider.
//!
//! The URL is hostile input. A tenant who can set it can aim the runner's
//! egress at a loopback admin port, the cloud metadata service, or an internal
//! RFC1918 host — Server-Side Request Forgery. This module refuses those at the
//! RESOLVE boundary, before a lease exists, so a blocked endpoint never reaches
//! the engine or the egress allowlist.
//!
//! # Two rules, and they are one function
//!
//! The Zig splits them across two files: `base_url_guard.validate` answers
//! whether a URL is safe, and `secret_probe.validateSecretEndpoint` answers
//! whether this PROVIDER may carry one at all. Both are pure, both are always
//! called together, and the second is meaningless without the first — a named
//! provider that smuggles a `base_url` widens the egress allowlist without
//! going through the compatible path, which is a bypass rather than a typo.
//!
//! [`resolve`] is the pair, stated once. [`validate`] stays a separate function
//! rather than being inlined into it, because the egress allowlist needs the bare
//! HOST — which is the one thing the pairing rule discards — and that caller
//! lands in the sibling slice that builds the execution policy.
//!
//! # Why the host is extracted by hand, with `url` in the lock
//!
//! `url` 2.5.8 is already a transitive dependency and would parse this in one
//! line. It is not used, and the reason is a wire fact rather than a taste:
//! `Url::host_str` NORMALISES — it lower-cases, it punycodes an
//! internationalised name, and it returns an IPv6 literal UNBRACKETED. The host
//! this produces travels to a stock Zig runner as its egress-allowlist entry,
//! and `execution_policy.zig::hostFromUrl` produces the bracketed, unnormalised
//! form. A normalising parser here would put a different string on the wire
//! than the daemon this must stay interchangeable with, for endpoints nobody
//! would think to test. So the extraction is thirty lines that copy
//! `hostFromUrl`'s bytes, and the CLASSIFICATION — the part where being subtly
//! wrong is a security hole — is the standard library's (see [`super::ssrf`]).

use super::ssrf;

/// The only scheme a custom endpoint may use.
///
/// Plaintext `http` is refused outright rather than upgraded: the `api_key` sits
/// beside the URL in the same credential, and a downgraded dial puts it on the
/// wire in the clear.
const REQUIRED_SCHEME: &str = "https";

/// The scheme separator, and the width the host extraction skips past it.
const SCHEME_SEPARATOR: &str = "://";

/// The provider id that opts a credential into a custom OpenAI-compatible
/// endpoint.
///
/// The credential JSON's own `provider` value, and deliberately distinct from
/// the `custom:<url>` name the runner is handed on the wire — one names the
/// SHAPE of the credential, the other names the dial.
pub const OPENAI_COMPATIBLE: &str = "openai-compatible";

/// Why an endpoint was refused.
///
/// Carried as a reason rather than collapsed into a bool because the rejection
/// LOG names it, and an operator reading "blocked host" acts differently from
/// one reading "not https". Only the reason and the host are ever logged; the
/// `api_key` sitting beside the URL in the same credential is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The scheme is not `https` — plain http, another scheme, or none at all.
    InvalidScheme,
    /// The host is an IP literal in an SSRF-unsafe range.
    BlockedHost,
    /// There is no parseable authority to check.
    Malformed,
    /// A named provider carried an endpoint, which only the compatible
    /// provider may do.
    NotPermitted,
    /// The compatible provider carried no endpoint, which it must.
    Required,
}

impl Rejection {
    /// The word this rejection is logged under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidScheme => "invalid_scheme",
            Self::BlockedHost => "blocked_host",
            Self::Malformed => "malformed",
            Self::NotPermitted => "not_permitted",
            Self::Required => "required",
        }
    }
}

/// The endpoint `provider` may dial, given what its credential declared.
///
/// `Ok(None)` is a named provider with no endpoint, which is the ordinary case:
/// it dials a built-in host and has nothing to validate. `Ok(Some(url))` is a
/// compatible provider whose endpoint passed every check, borrowed from the
/// input for the caller to own.
///
/// # Errors
/// Refuses a named provider that carries an endpoint, a compatible provider
/// that carries none, and any endpoint [`validate`] rejects.
pub(super) fn resolve<'a>(
    provider: &str,
    base_url: Option<&'a str>,
) -> Result<Option<&'a str>, Rejection> {
    if provider != OPENAI_COMPATIBLE {
        return base_url.map_or(Ok(None), |_smuggled| Err(Rejection::NotPermitted));
    }
    let url = base_url.ok_or(Rejection::Required)?;
    validate(url).map(|_host| Some(url))
}

/// The bare host of a safe `https` endpoint.
///
/// Order matters and is the Zig's: scheme first because it is the cheapest
/// check and an `http` URL is refused whatever its host, then the authority,
/// then the SSRF classification.
///
/// # Errors
/// Refuses a non-`https` scheme, an absent or unparseable authority, and a host
/// that is an SSRF-unsafe IP literal.
pub(super) fn validate(url: &str) -> Result<&str, Rejection> {
    let (scheme, authority) = url
        .split_once(SCHEME_SEPARATOR)
        .ok_or(Rejection::InvalidScheme)?;
    if !scheme.eq_ignore_ascii_case(REQUIRED_SCHEME) {
        return Err(Rejection::InvalidScheme);
    }

    let host = host_of(authority).ok_or(Rejection::Malformed)?;
    if ssrf::is_blocked_literal(host) {
        return Err(Rejection::BlockedHost);
    }
    Ok(host)
}

/// The bare host inside an authority, with userinfo, port and path removed.
///
/// Byte-for-byte what `hostFromUrl` produces, bracketed IPv6 included — see the
/// module note on why that spelling is preserved rather than normalised.
///
/// `None` is an authority with nothing left after the strips: `https:///path`,
/// `https://user@`, or an IPv6 literal whose bracket never closes.
fn host_of(after_scheme: &str) -> Option<&str> {
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|section| !section.is_empty())?;

    // `rsplit_once`, not `split_once`: a smuggled `user@evil` must not let the
    // LAST authority component masquerade as userinfo and hand `evil` back as
    // the host.
    let after_userinfo = authority
        .rsplit_once('@')
        .map_or(authority, |(_userinfo, host)| host);

    if after_userinfo.starts_with('[') {
        // The inner colons are address bytes rather than a port, so the close
        // bracket is what ends the host — and its absence is malformed rather
        // than a host that happens to contain a bracket. `get`, not a range
        // index: a slice that panics is not a parser (`clippy::indexing_slicing`).
        return after_userinfo.get(..=after_userinfo.find(']')?);
    }
    after_userinfo
        .split(':')
        .next()
        .filter(|host| !host.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{OPENAI_COMPATIBLE, Rejection, resolve, validate};

    #[test]
    fn a_public_https_endpoint_yields_its_bare_host() {
        assert_eq!(
            validate("https://api.openrouter.ai/v1"),
            Ok("api.openrouter.ai")
        );
        // Port, path and userinfo are all stripped down to the host.
        assert_eq!(
            validate("https://user:pw@gw.example.com:8443/v1"),
            Ok("gw.example.com")
        );
        // https is matched case-insensitively, as a scheme is.
        assert_eq!(validate("HTTPS://api.example.com"), Ok("api.example.com"));
        // A bracketed v6 literal keeps its brackets — the wire-parity property
        // the module note is about.
        assert_eq!(
            validate("https://[2606:4700:4700::1111]/v1"),
            Ok("[2606:4700:4700::1111]")
        );
    }

    #[test]
    fn a_smuggled_userinfo_cannot_hide_an_internal_target() {
        // `evil.com` is the USERINFO here and the metadata service is the host.
        // Taking the first `@` instead of the last would invert that and pass.
        assert_eq!(
            validate("https://evil.com@169.254.169.254/v1"),
            Err(Rejection::BlockedHost)
        );
    }

    #[test]
    fn only_https_is_accepted() {
        for refused in [
            "http://api.example.com/v1",
            "ws://api.example.com",
            "file:///etc/passwd",
            "api.example.com/v1",
            "HTTP://api.example.com",
        ] {
            assert_eq!(
                validate(refused),
                Err(Rejection::InvalidScheme),
                "{refused}"
            );
        }
    }

    #[test]
    fn an_authority_that_is_not_there_is_malformed() {
        for refused in [
            "https:///just/a/path",
            "https://",
            "https://[::1",
            "https://user@",
            "https://:8443/v1",
        ] {
            assert_eq!(validate(refused), Err(Rejection::Malformed), "{refused}");
        }
    }

    #[test]
    fn the_ssrf_ranges_are_refused_before_a_lease_exists() {
        for refused in [
            "https://127.0.0.1/v1",
            "https://10.1.2.3/v1",
            "https://192.168.1.1/v1",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]:8443/v1",
            "https://[::ffff:169.254.169.254]/v1",
        ] {
            assert_eq!(validate(refused), Err(Rejection::BlockedHost), "{refused}");
        }
    }

    #[test]
    fn a_named_provider_carries_no_endpoint_and_may_not_smuggle_one() {
        assert_eq!(resolve("anthropic", None), Ok(None));
        // The bypass this rule exists for: a named provider with a base_url
        // would widen the egress allowlist without going through the
        // compatible path at all.
        assert_eq!(
            resolve("anthropic", Some("https://api.example.com")),
            Err(Rejection::NotPermitted)
        );
    }

    #[test]
    fn the_compatible_provider_must_carry_a_valid_endpoint() {
        assert_eq!(
            resolve(OPENAI_COMPATIBLE, Some("https://gw.example.com/v1")),
            Ok(Some("https://gw.example.com/v1"))
        );
        assert_eq!(resolve(OPENAI_COMPATIBLE, None), Err(Rejection::Required));
        // And the safety checks still apply through the pairing rule, not only
        // when `validate` is called directly.
        assert_eq!(
            resolve(OPENAI_COMPATIBLE, Some("https://127.0.0.1/v1")),
            Err(Rejection::BlockedHost)
        );
        assert_eq!(
            resolve(OPENAI_COMPATIBLE, Some("http://gw.example.com/v1")),
            Err(Rejection::InvalidScheme)
        );
    }

    #[test]
    fn every_rejection_is_logged_under_a_distinct_word() {
        let reasons = [
            Rejection::InvalidScheme,
            Rejection::BlockedHost,
            Rejection::Malformed,
            Rejection::NotPermitted,
            Rejection::Required,
        ]
        .map(Rejection::as_str);
        for (index, reason) in reasons.iter().enumerate() {
            assert!(!reason.is_empty());
            assert!(
                !reasons.iter().skip(index + 1).any(|other| other == reason),
                "`{reason}` is spelled twice, so a log cannot say which fired"
            );
        }
    }
}
