//! Which hosts a tenant may not point an endpoint at.
//!
//! # What this replaces
//!
//! `ip_literal.zig` is two hundred and forty-nine lines: an IPv4 parser, an
//! IPv6 parser with its own `::` elision handling and embedded-IPv4 case, a
//! colon-run helper, and fourteen named octet constants the range checks are
//! spelled against. Every line of it is re-deriving something the standard
//! library already does — and re-deriving it in the one place where being
//! subtly wrong is a Server-Side Request Forgery, because a literal a
//! classifier fails to PARSE is a literal it reports as safe.
//!
//! Nothing here parses at all now. [`super::endpoint`] hands over a
//! [`Host`] the URL parser already resolved, so an IP literal arrives as an
//! [`Ipv4Addr`] or an [`Ipv6Addr`] and a name arrives as a name. Bracket
//! stripping, zone-id handling and the "is this even an address" question are
//! all upstream, in a parser that is maintained.
//!
//! # The four predicates std does not answer, and why they are hand-written
//!
//! - `0.0.0.0/8` — `Ipv4Addr::is_unspecified` is `0.0.0.0` EXACTLY, and the Zig
//!   blocks the whole `/8`. One octet comparison.
//! - `240.0.0.0/4` — reserved, not multicast, so `is_multicast` misses it while
//!   the Zig's `>= 224` catches it. Folded into one comparison with multicast
//!   and broadcast, which is how the Zig spells it too.
//! - `fc00::/7` — `Ipv6Addr::is_unique_local` is unstable.
//! - `fe80::/10` — `Ipv6Addr::is_unicast_link_local` is unstable.
//!
//! Both IPv6 predicates are one masked comparison against the first segment.
//! When they stabilise these four lines go away; until then they are named
//! here rather than left as a gap.
//!
//! # Scope, unchanged from the Zig
//!
//! HOST-LITERAL classification only, mirroring the runner's vendored
//! `nullclaw/net_security.zig`. A hostname that RESOLVES to a private address
//! is not caught here and is not meant to be — the runner re-checks after
//! resolution at connect time, and keeping the control-plane verdict and the
//! data-plane enforcement on the same predicates is what stops the two
//! disagreeing.

use std::net::{Ipv4Addr, Ipv6Addr};

use url::Host;

/// First octet of `0.0.0.0/8` — "this host", blocked as a whole range.
const V4_UNSPECIFIED_BLOCK: u8 = 0;

/// First octet at which IPv4 stops being unicast: multicast `224/4` through
/// reserved `240/4` to the broadcast address.
const V4_NON_UNICAST_FLOOR: u8 = 224;

/// `fc00::/7` unique-local: first segment, masked and compared.
const V6_UNIQUE_LOCAL_MASK: u16 = 0xfe00;
/// See [`V6_UNIQUE_LOCAL_MASK`].
const V6_UNIQUE_LOCAL: u16 = 0xfc00;

/// `fe80::/10` link-local: first segment, masked and compared.
const V6_LINK_LOCAL_MASK: u16 = 0xffc0;
/// See [`V6_LINK_LOCAL_MASK`].
const V6_LINK_LOCAL: u16 = 0xfe80;

/// Whether `host` is an address a tenant endpoint may not reach.
///
/// A DOMAIN answers `false`: it is a name, and names are the runner's to
/// re-check after resolution. That is not a gap this module should close —
/// resolving here and trusting the answer is a DNS-rebinding hole, because the
/// name can resolve differently when the runner dials it.
pub(super) fn is_blocked(host: &Host<&str>) -> bool {
    match host {
        Host::Ipv4(address) => is_blocked_v4(*address),
        Host::Ipv6(address) => is_blocked_v6(*address),
        Host::Domain(_name) => false,
    }
}

/// The IPv4 blocklist: loopback, RFC1918, link-local, `0/8`, and everything
/// from multicast up.
///
/// Documentation, shared-address and benchmarking ranges are deliberately
/// absent, exactly as they are in the Zig. They are globally unroutable but
/// they are not an SSRF target, and a tenant may legitimately front a real
/// gateway inside one.
fn is_blocked_v4(address: Ipv4Addr) -> bool {
    let first = address.octets()[0];
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || first == V4_UNSPECIFIED_BLOCK
        || first >= V4_NON_UNICAST_FLOOR
}

/// The IPv6 blocklist, plus the IPv4-mapped form of anything above.
///
/// The mapped case is the one an attacker reaches for: `::ffff:169.254.169.254`
/// is the cloud metadata service wearing a v6 spelling, and a classifier that
/// checked only the v6 ranges would pass it.
fn is_blocked_v6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_blocked_v4(mapped);
    }
    let first = address.segments()[0];
    address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || first & V6_UNIQUE_LOCAL_MASK == V6_UNIQUE_LOCAL
        || first & V6_LINK_LOCAL_MASK == V6_LINK_LOCAL
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::is_blocked;
    use url::Host;

    /// The host of `raw`, as the guard's parser resolves it.
    fn host_of(raw: &str) -> Host<String> {
        url::Url::parse(raw)
            .expect("a parseable URL")
            .host()
            .expect("a URL with an authority")
            .to_owned()
    }

    /// Whether the host of `raw` is refused.
    fn blocks(raw: &str) -> bool {
        let owned = host_of(raw);
        let borrowed = match &owned {
            Host::Domain(name) => Host::Domain(name.as_str()),
            Host::Ipv4(address) => Host::Ipv4(*address),
            Host::Ipv6(address) => Host::Ipv6(*address),
        };
        is_blocked(&borrowed)
    }

    /// Every range `ip_literal.zig` blocks, in the spellings its own suite uses
    /// — that suite, re-run against a real URL parser.
    #[test]
    fn the_v4_blocklist_matches_the_zig_ranges() {
        for blocked in [
            "127.0.0.1",
            "127.255.255.255",
            "10.0.0.0",
            "10.255.255.255",
            "172.16.5.9",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
        ] {
            assert!(
                blocks(&format!("https://{blocked}/v1")),
                "{blocked} must be blocked"
            );
        }
    }

    #[test]
    fn the_v4_boundaries_that_are_public_are_not_over_blocked() {
        // A /12 or /16 widened by one octet is how an SSRF guard quietly stops
        // a tenant from reaching their own gateway.
        for allowed in [
            "172.15.0.1",
            "172.32.0.1",
            "169.253.0.1",
            "169.255.0.1",
            "8.8.8.8",
            "1.1.1.1",
        ] {
            assert!(
                !blocks(&format!("https://{allowed}/v1")),
                "{allowed} must be allowed"
            );
        }
    }

    #[test]
    fn the_v6_blocklist_covers_every_range_and_the_mapped_forms() {
        for blocked in [
            "[::1]",
            "[::]",
            "[fc00::1]",
            "[fd12::3]",
            "[fe80::1]",
            "[ff02::1]",
            "[::ffff:127.0.0.1]",
            "[::ffff:169.254.169.254]",
        ] {
            assert!(
                blocks(&format!("https://{blocked}/v1")),
                "{blocked} must be blocked"
            );
        }
        assert!(
            !blocks("https://[2606:4700:4700::1111]/v1"),
            "a public v6 resolver must stay reachable"
        );
    }

    #[test]
    fn a_hostname_is_not_classified_here() {
        // Names are the runner's to re-check after resolution — see the module
        // note. Resolving here and trusting it is a DNS-rebinding hole.
        for allowed in [
            "example.com",
            "self-hosted.vllm.internal-corp.net",
            // Numeric-looking but not four numeric parts, so the parser
            // reads it as a domain — and it resolves like any other name.
            "10.0.0.0.example.com",
        ] {
            assert!(
                !blocks(&format!("https://{allowed}/v1")),
                "{allowed} is a name"
            );
        }
    }
}
