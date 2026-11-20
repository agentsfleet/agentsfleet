//! The guard every HTTP client of the generic tools carries: a host is reached
//! only when it is not, and does not resolve to, a private address.
//!
//! ```text
//!  url ─► an address literal? ─► judged where the URL is admitted; it never
//!   │                            reaches a lookup
//!   └──► a name ─► the lookup ─► every address it answers judged ─► connect
//! ```
//!
//! Private here is every address a model's request must not reach from the
//! host it runs on: loopback, the private ranges, link-local, unique-local,
//! the shared address space carriers and tailnets number their peers from
//! (CGNAT), unspecified, multicast and the reserved ranges, each in its IPv6
//! spellings too. One private answer refuses the name whole, so a name that
//! points inside and out is never reached at its public address either.
//!
//! The judgment runs in the client's resolver, on every connection's own
//! lookup, so a redirect's host and a name whose answer changed after it was
//! admitted are judged on the answer the connection would use. The lookup
//! itself is the embedder's to replace, such as a test's fixed answers; the
//! judgment of its answer is not.

use std::error::Error as StdError;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::{Host, Url};

use super::Refused;
use crate::tools::code;

/// The port a lookup is made for: any, since reqwest sets the URL's own.
const ANY_PORT: u16 = 0;
/// What the model reads after an address literal the guard refuses.
const LITERAL_DENIED: &str = "is a private, loopback or reserved address";
/// What the model reads after a name the guard refuses.
const NAME_DENIED: &str = "resolves to a private, loopback or reserved address";

/// First octet of `0.0.0.0/8`, "this host", refused as a whole range.
const V4_UNSPECIFIED_BLOCK: u8 = 0;
/// The first three octets of `192.0.0.0/24`, the IETF protocol assignments.
const V4_PROTOCOL_ASSIGNMENTS: [u8; 3] = [192, 0, 0];
/// First octet at which IPv4 stops being unicast: multicast `224/4` through
/// reserved `240/4` to the broadcast address.
const V4_NON_UNICAST_FLOOR: u8 = 224;
/// `100.64.0.0/10`, the shared address space: its first octet, then the
/// second octet masked and compared.
const V4_SHARED_FIRST: u8 = 100;
/// See [`V4_SHARED_FIRST`].
const V4_SHARED_MASK: u8 = 0xc0;
/// See [`V4_SHARED_FIRST`].
const V4_SHARED_SECOND: u8 = 64;
/// `fc00::/7` unique-local: the first segment, masked and compared.
const V6_UNIQUE_LOCAL_MASK: u16 = 0xfe00;
/// See [`V6_UNIQUE_LOCAL_MASK`].
const V6_UNIQUE_LOCAL: u16 = 0xfc00;
/// `fe80::/10` link-local: the first segment, masked and compared.
const V6_LINK_LOCAL_MASK: u16 = 0xffc0;
/// See [`V6_LINK_LOCAL_MASK`].
const V6_LINK_LOCAL: u16 = 0xfe80;
/// The first two segments of NAT64's prefixes, `64:ff9b::/96` and the
/// local-use `64:ff9b:1::/48`.
const V6_NAT64: [u16; 2] = [0x0064, 0xff9b];
/// The third segment that makes a NAT64 prefix the local-use one.
const V6_NAT64_LOCAL_USE: u16 = 0x0001;
/// The first segment of 6to4, `2002::/16`.
const V6_6TO4: u16 = 0x2002;
/// The sixth segment of an IPv4-mapped address, `::ffff:0:0/96`.
const V6_MAPPED: u16 = 0xffff;

/// Whether a request may not reach `address`.
///
/// Documentation and benchmarking ranges are left reachable: they route
/// nowhere, and a gateway may legitimately sit in one.
pub(crate) fn is_denied(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_denied_v4(address),
        IpAddr::V6(address) => is_denied_v6(address),
    }
}

/// Loopback, the private ranges, the shared address space, link-local, the
/// IETF protocol assignments, `0/8`, and everything from multicast up.
fn is_denied_v4(address: Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();
    address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || (first == V4_SHARED_FIRST && second & V4_SHARED_MASK == V4_SHARED_SECOND)
        || [first, second, third] == V4_PROTOCOL_ASSIGNMENTS
        || first == V4_UNSPECIFIED_BLOCK
        || first >= V4_NON_UNICAST_FLOOR
}

/// The IPv6 ranges, and every IPv6 spelling of an IPv4 address: the cloud
/// metadata service written `::ffff:169.254.169.254` is still the metadata
/// service. NAT64's local-use prefix carries its IPv4 address in a layout its
/// operator picks, so the whole prefix is refused.
fn is_denied_v6(address: Ipv6Addr) -> bool {
    if let Some(embedded) = embedded_v4(address) {
        return is_denied_v4(embedded);
    }
    let [first, second, third, ..] = address.segments();
    address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || first & V6_UNIQUE_LOCAL_MASK == V6_UNIQUE_LOCAL
        || first & V6_LINK_LOCAL_MASK == V6_LINK_LOCAL
        || ([first, second] == V6_NAT64 && third == V6_NAT64_LOCAL_USE)
}

/// The IPv4 address an IPv6 spelling routes to: IPv4-mapped,
/// IPv4-compatible, NAT64's well-known prefix, and 6to4.
fn embedded_v4(address: Ipv6Addr) -> Option<Ipv4Addr> {
    let [prefix @ .., high, low] = address.segments();
    let join = |high: u16, low: u16| Ipv4Addr::from((u32::from(high) << 16) | u32::from(low));
    match prefix {
        [0, 0, 0, 0, 0, 0 | V6_MAPPED] => Some(join(high, low)),
        [nat_high, nat_low, 0, 0, 0, 0] if [nat_high, nat_low] == V6_NAT64 => Some(join(high, low)),
        [V6_6TO4, tunnel_high, tunnel_low, ..] => Some(join(tunnel_high, tunnel_low)),
        _ => None,
    }
}

/// A host the guard refused: a private address literal, or a name one of
/// whose addresses is private. A client's resolver hands it to reqwest, and
/// [`denied`] reads it back out of the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAddress {
    detail: String,
}

impl PrivateAddress {
    /// `host`, an address literal.
    fn literal(host: &str) -> Self {
        Self {
            detail: format!("{host} {LITERAL_DENIED}"),
        }
    }

    /// `host`, a name that resolves to a private address.
    fn resolved(host: &str) -> Self {
        Self {
            detail: format!("{host} {NAME_DENIED}"),
        }
    }
}

impl fmt::Display for PrivateAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl StdError for PrivateAddress {}

/// The refusal the model reads: [`code::EGRESS_DENIED`] and the host.
impl From<PrivateAddress> for Refused {
    fn from(denied: PrivateAddress) -> Self {
        Self::new(code::EGRESS_DENIED, denied.detail)
    }
}

/// `url`, unless its host is a private address literal. A name passes: it is
/// judged by the client's resolver as it connects.
pub(crate) fn literal_admitted(url: &Url) -> Result<(), PrivateAddress> {
    let address: Option<IpAddr> = match url.host() {
        Some(Host::Ipv4(address)) => Some(address.into()),
        Some(Host::Ipv6(address)) => Some(address.into()),
        Some(Host::Domain(_)) | None => None,
    };
    if address.is_some_and(is_denied) {
        Err(PrivateAddress::literal(url.host_str().unwrap_or_default()))
    } else {
        Ok(())
    }
}

/// `url`'s host judged before a client that speaks to it itself connects: a
/// literal as it stands, a name by every address `lookup` answers for it.
///
/// The client still carries the guard, so a name whose answer changes before
/// it connects is judged again on the answer it would use.
///
/// # Errors
/// [`code::EGRESS_DENIED`] for a private host; [`code::INVALID_ARGUMENTS`]
/// for a host no lookup takes; [`code::UPSTREAM_UNREACHABLE`] when the
/// lookup failed.
pub(crate) async fn reachable(url: &Url, lookup: &dyn Resolve) -> Result<(), Refused> {
    literal_admitted(url)?;
    let Some(Host::Domain(host)) = url.host() else {
        return Ok(());
    };
    let name: Name = host.parse().map_err(|invalid| {
        Refused::new(
            code::INVALID_ARGUMENTS,
            format!("{host} is not a host name: {invalid}"),
        )
    })?;
    match judged(host.to_owned(), lookup.resolve(name)).await {
        Ok(_addresses) => Ok(()),
        Err(failure) => Err(denied(&*failure).cloned().map_or_else(
            || {
                Refused::new(
                    code::UPSTREAM_UNREACHABLE,
                    format!("{host} could not be looked up: {failure}"),
                )
            },
            Refused::from,
        )),
    }
}

/// The guard's refusal somewhere among `failure`'s causes, when it refused.
pub(crate) fn denied<'a>(failure: &'a (dyn StdError + 'static)) -> Option<&'a PrivateAddress> {
    std::iter::successors(Some(failure), |&cause| cause.source())
        .find_map(<dyn StdError>::downcast_ref::<PrivateAddress>)
}

/// This host's own resolver: a name looked up as every other program on the
/// host looks it up.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLookup;

impl Resolve for SystemLookup {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found = tokio::net::lookup_host((host, ANY_PORT)).await?;
            Ok(Box::new(found) as Addrs)
        })
    }
}

/// A lookup whose every answer is judged: a name with any private address
/// is refused whole, before a connection is opened to any of them.
pub(crate) struct Guarded {
    lookup: Arc<dyn Resolve>,
}

impl Guarded {
    /// Judges what `lookup` answers.
    pub(crate) fn new(lookup: Arc<dyn Resolve>) -> Self {
        Self { lookup }
    }
}

impl Resolve for Guarded {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(judged(host, self.lookup.resolve(name)))
    }
}

/// What a lookup of `host` answered, unless any address in it is private.
async fn judged(host: String, answer: Resolving) -> Result<Addrs, Unresolved> {
    let addresses: Vec<SocketAddr> = answer.await?.collect();
    if addresses.iter().any(|address| is_denied(address.ip())) {
        return Err(PrivateAddress::resolved(&host).into());
    }
    Ok(Box::new(addresses.into_iter()))
}

/// What a resolver hands reqwest when it fails or refuses.
type Unresolved = Box<dyn StdError + Send + Sync>;

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
