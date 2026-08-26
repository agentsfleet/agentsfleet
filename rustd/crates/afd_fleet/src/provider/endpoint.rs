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
//! # The parse is `url`'s, not thirty hand-written lines
//!
//! `base_url_guard.zig` scans for `://`, finds the first `/?#`, takes the last
//! `@`, and looks for a `:` unless the authority opens with `[` — a URL parser,
//! written by hand, in the one place where disagreeing with the client that
//! will actually dial the address is a security hole. This uses `url` 2.5.8,
//! already in the lock, and gets a TYPED [`Host`] out of it: an IP literal
//! arrives as an `Ipv4Addr` or `Ipv6Addr` and goes straight to the classifier
//! with no string round trip, and bracket stripping, zone ids, percent-encoded
//! authorities and userinfo are all handled by something maintained.
//!
//! The one thing kept by hand is how an IPv6 host is RENDERED back. `url` hands
//! it over unbracketed and `execution_policy.zig::hostFromUrl` produces the
//! bracketed form, and this value travels to a stock Zig runner as its
//! egress-allowlist entry. Normalisation is otherwise safe here, which was
//! checked rather than assumed: the runner compares allowlist entries with
//! `std.ascii.eqlIgnoreCase` at all three of its matching sites, so a
//! lower-cased host still matches.
//!
//! # Three verdicts differ from the hand-written guard, and all three are it
//!
//! Every one is a case where the Zig disagrees with what an HTTP client would
//! actually dial, which is the only thing this guard is for — a verdict about a
//! host nobody will connect to protects nothing.
//!
//! - `https:///just/a/path` — the Zig calls it malformed. A client following
//!   WHATWG skips the extra slash and dials `just`, and so does this. The SSRF
//!   check still runs on that host, so `https:///169.254.169.254` is refused
//!   exactly as the two-slash spelling is.
//! - `https://256.1.1.1/v1` — the Zig's `parseIpv4` fails, concludes "not a
//!   literal", and passes it through as a NAME. WHATWG reads a four-part
//!   all-numeric host as an IPv4 attempt and refuses the out-of-range octet, so
//!   this refuses it too. The safe direction, and the one a client takes.
//! - A schemeless host reports `InvalidScheme` rather than the parser's own
//!   "relative URL" — the diagnosis an operator can act on, and the Zig's.

use url::{Host, Url};

use super::ssrf;

/// The only scheme a custom endpoint may use.
///
/// Plaintext `http` is refused outright rather than upgraded: the `api_key` sits
/// beside the URL in the same credential, and a downgraded dial puts it on the
/// wire in the clear.
///
/// Compared against `Url::scheme`, which the parser has already lower-cased —
/// so `HTTPS://` matches without this doing its own case folding.
const REQUIRED_SCHEME: &str = "https";

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
/// check and an `http` URL is refused whatever its host, then the SSRF
/// classification of whatever the parser resolved.
///
/// Owned rather than borrowed from the input, because the parser normalises and
/// the result is no longer a slice of what came in. That is the honest
/// signature: a borrow would have implied the bytes are the caller's.
///
/// # Errors
/// Refuses a URL that does not parse, a non-`https` scheme, an authority with
/// no host, and a host that is an SSRF-unsafe address.
pub(super) fn validate(url: &str) -> Result<Box<str>, Rejection> {
    let parsed = Url::parse(url).map_err(|failure| match failure {
        // A schemeless host is the operator who wrote `api.example.com` and
        // forgot the `https://`. The parser calls that a relative URL; the
        // rejection they need to read says the scheme is the problem.
        url::ParseError::RelativeUrlWithoutBase => Rejection::InvalidScheme,
        _malformed => Rejection::Malformed,
    })?;
    if parsed.scheme() != REQUIRED_SCHEME {
        return Err(Rejection::InvalidScheme);
    }
    // `Some` for every hierarchical scheme; `None` is a URL with no authority
    // at all, which `https` cannot legally be but the parser still models.
    let host = parsed.host().ok_or(Rejection::Malformed)?;
    if ssrf::is_blocked(&host) {
        return Err(Rejection::BlockedHost);
    }
    Ok(render(&host))
}

/// The host as the egress allowlist spells it.
///
/// An IPv6 literal is re-bracketed, because that is what
/// `execution_policy.zig::hostFromUrl` produces and the value goes to a stock
/// Zig runner. Everything else is the parser's own rendering.
fn render(host: &Host<&str>) -> Box<str> {
    match host {
        Host::Ipv6(address) => format!("[{address}]").into_boxed_str(),
        Host::Ipv4(address) => address.to_string().into_boxed_str(),
        Host::Domain(name) => (*name).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{OPENAI_COMPATIBLE, Rejection, resolve, validate};

    /// The host `raw` validates to, as an owned string.
    fn host(raw: &str) -> Result<String, Rejection> {
        validate(raw).map(str::into_string)
    }

    #[test]
    fn a_public_https_endpoint_yields_its_bare_host() {
        assert_eq!(
            host("https://api.openrouter.ai/v1").as_deref(),
            Ok("api.openrouter.ai")
        );
        // Port, path and userinfo are all stripped down to the host.
        assert_eq!(
            host("https://user:pw@gw.example.com:8443/v1").as_deref(),
            Ok("gw.example.com")
        );
        // The scheme is matched case-insensitively, because the parser has
        // already lower-cased it.
        assert_eq!(
            host("HTTPS://api.example.com").as_deref(),
            Ok("api.example.com")
        );
        // A v6 literal is re-bracketed — the wire-parity property the module
        // note is about, and the one thing rendering keeps by hand.
        assert_eq!(
            host("https://[2606:4700:4700::1111]/v1").as_deref(),
            Ok("[2606:4700:4700::1111]")
        );
    }

    #[test]
    fn a_host_is_normalised_the_way_the_runner_already_compares() {
        // Checked rather than assumed: the runner matches allowlist entries
        // with `eqlIgnoreCase` at all three of its sites, so lower-casing here
        // cannot make a legitimate endpoint unreachable.
        assert_eq!(host("https://Example.COM/v1").as_deref(), Ok("example.com"));
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
            "https://",
            "https://[::1",
            "https://user@",
            "https://:8443/v1",
        ] {
            assert_eq!(validate(refused), Err(Rejection::Malformed), "{refused}");
        }
    }

    #[test]
    fn a_url_is_judged_as_a_client_would_dial_it() {
        // An extra slash is skipped by every WHATWG client, so the host is
        // `just` and this guard says so — where the hand-written parser called
        // it malformed and protected nothing, because a client would still
        // have connected.
        assert_eq!(host("https:///just/a/path").as_deref(), Ok("just"));
        // And the SSRF check runs on THAT host, so the spelling buys no bypass.
        assert_eq!(
            validate("https:///169.254.169.254/latest"),
            Err(Rejection::BlockedHost)
        );
        // A four-part all-numeric host is an IPv4 attempt, not a name. The
        // hand-written parser failed to parse it and concluded it was a
        // hostname, which is the unsafe direction.
        assert_eq!(validate("https://256.1.1.1/v1"), Err(Rejection::Malformed));
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
