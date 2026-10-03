//! Which addresses an outbound request on a tenant's behalf may not reach.
//!
//! Two planes ask the same question. `agentsfleetd` refuses an endpoint a
//! tenant points at an internal address before any lease exists, and the
//! runner refuses every address a name resolves to before it dials. Keeping
//! both on one predicate is what stops the control-plane verdict and the
//! data-plane enforcement from disagreeing.
//!
//! # The four ranges std does not answer, and why they are hand-written
//!
//! - `0.0.0.0/8` — `Ipv4Addr::is_unspecified` is `0.0.0.0` EXACTLY, and the
//!   whole `/8` is blocked. One octet comparison.
//! - `240.0.0.0/4` — reserved, not multicast, so `is_multicast` misses it.
//!   Folded into one comparison with multicast and broadcast.
//! - `fc00::/7` — `Ipv6Addr::is_unique_local` is unstable.
//! - `fe80::/10` — `Ipv6Addr::is_unicast_link_local` is unstable.
//!
//! Both IPv6 predicates are one masked comparison against the first segment.
//! When they stabilise these lines go away.
//!
//! # What is deliberately absent
//!
//! Documentation, shared-address and benchmarking ranges are globally
//! unroutable but they are not a Server-Side Request Forgery target, and a
//! tenant may legitimately front a real gateway inside one.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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

/// Whether a request on a tenant's behalf may not reach `address`.
#[must_use]
pub fn is_blocked(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_blocked_v4(address),
        IpAddr::V6(address) => is_blocked_v6(address),
    }
}

/// Loopback, RFC1918, link-local, `0/8`, and everything from multicast up.
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
#[path = "net/tests.rs"]
mod tests;
