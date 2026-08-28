//! Is this URL one the daemon will let a run dial?
//!
//! Split from the pairing rule beside it because the two answer different
//! questions and fail for different reasons. [`super::resolve`] asks whether
//! this PROVIDER may carry an endpoint at all; this asks whether a given URL is
//! safe to reach. A credential can satisfy either and fail the other.
//!
//! Order inside the check is the Zig's and is deliberate: scheme first because
//! it is the cheapest and an `http` URL is refused whatever its host, then the
//! SSRF classification of whatever the parser actually resolved.

use url::{Host, Url};

use super::Rejection;
use crate::provider::ssrf;

/// The only scheme a custom endpoint may use.
///
/// Plaintext `http` is refused outright rather than upgraded: the `api_key` sits
/// beside the URL in the same credential, and a downgraded dial puts it on the
/// wire in the clear.
///
/// Compared against `Url::scheme`, which the parser has already lower-cased —
/// so `HTTPS://` matches without this doing its own case folding.
const REQUIRED_SCHEME: &str = "https";

pub(crate) fn validate(url: &str) -> Result<Box<str>, Rejection> {
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
    use super::validate;
    use crate::provider::endpoint::{Endpoint, OPENAI_COMPATIBLE, Rejection, resolve};

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
            Ok(Some(Endpoint {
                url: "https://gw.example.com/v1",
                // The host comes back BESIDE the url, so nothing downstream
                // re-derives what the SSRF ruling was made against.
                host: "gw.example.com".into(),
            }))
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
