//! Which hosts a tenant may not point an endpoint at.
//!
//! [`super::endpoint`] hands over a [`Host`] the URL parser already resolved,
//! so an IP literal arrives as an address and a name arrives as a name. The
//! ranges themselves are [`afd_core::net::is_blocked`]'s, the predicate the
//! runner applies to every address a name resolves to before it dials, so the
//! control-plane verdict and the data-plane enforcement cannot disagree.
//!
//! HOST-LITERAL classification only. A hostname that RESOLVES to a private
//! address is not caught here and is not meant to be: resolving here and
//! trusting the answer is a DNS-rebinding hole, because the name can resolve
//! differently when the runner dials it.

use std::net::IpAddr;

use url::Host;

/// Whether `host` is an address a tenant endpoint may not reach; a domain
/// answers `false`.
pub(super) fn is_blocked(host: &Host<&str>) -> bool {
    match host {
        Host::Ipv4(address) => afd_core::net::is_blocked(IpAddr::V4(*address)),
        Host::Ipv6(address) => afd_core::net::is_blocked(IpAddr::V6(*address)),
        Host::Domain(_name) => false,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::is_blocked;
    use url::Host;

    fn blocks(host: &str) -> bool {
        let owned = Host::parse(host).expect("a parseable host");
        is_blocked(&match &owned {
            Host::Domain(name) => Host::Domain(name.as_str()),
            Host::Ipv4(address) => Host::Ipv4(*address),
            Host::Ipv6(address) => Host::Ipv6(*address),
        })
    }

    #[test]
    fn test_parsed_literals_reach_the_shared_ranges() {
        // The ranges are `afd_core::net`'s suite; this pins that both literal
        // forms the URL parser produces are handed to it, brackets stripped.
        for blocked in ["169.254.169.254", "[::1]", "[::ffff:10.0.0.1]"] {
            assert!(blocks(blocked), "{blocked} must be blocked");
        }
        assert!(!blocks("8.8.8.8"), "a public literal must be allowed");
    }

    #[test]
    fn test_a_hostname_is_not_classified_here() {
        // Names are the runner's to re-check after resolution — see the module
        // note. Resolving here and trusting it is a DNS-rebinding hole.
        for allowed in [
            "example.com",
            "self-hosted.vllm.internal-corp.net",
            // Numeric-looking but not four numeric parts, so the parser reads
            // it as a domain — and it resolves like any other name.
            "10.0.0.0.example.com",
        ] {
            assert!(!blocks(allowed), "{allowed} is a name");
        }
    }
}
