//! How egress resolves a name, and the one rule every egress path holds the
//! answer to: a name any of whose addresses is blocked is refused whole.
//!
//! Two paths read it. The supervisor resolves `network.allow` into the kernel
//! set a lease's sandbox reaches (`afr_supervisor` `egress.rs`), and the
//! guarded client resolves each connection `http_request` or a model provider
//! makes ([`crate::guarded`]). Both resolve through [`Resolve`] and judge the
//! answer with [`unblocked`], so a host one path refuses is never a host the
//! other reaches.

use std::fmt;
use std::io;
use std::net::IpAddr;

use afd_core::net::is_blocked;

use crate::error::Result;

/// The port a lookup is made for: any, since only the address is kept.
const ANY_PORT: u16 = 0;

/// Resolves a host name to its addresses.
#[async_trait::async_trait]
pub trait Resolve: Send + Sync + fmt::Debug {
    /// Every address `host` resolves to.
    ///
    /// # Errors
    /// The resolver could not answer for `host`.
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>>;
}

/// The host's own resolver, as every other program on it resolves.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

#[async_trait::async_trait]
impl Resolve for SystemResolver {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let found = tokio::net::lookup_host((host, ANY_PORT)).await?;
        Ok(found.map(|socket| socket.ip()).collect())
    }
}

/// A name refused because one of its addresses is blocked; the guarded client
/// hands it to reqwest, and [`crate::blocked_address`] reads it back out.
#[derive(Debug, thiserror::Error)]
#[error("the name resolves to an address this runner never reaches")]
pub struct BlockedAddress;

/// `addresses`, one name's whole answer, unless any of them is blocked.
///
/// Blocked is `afd_core::net`'s ranges: loopback, private, shared, link-local
/// or reserved, an IPv4-mapped IPv6 spelling included. One blocked answer
/// means the name points inside, so the name is refused whole rather than
/// reached at its other addresses.
///
/// # Errors
/// [`BlockedAddress`], when any of `addresses` is blocked.
pub fn unblocked(addresses: Vec<IpAddr>) -> Result<Vec<IpAddr>, BlockedAddress> {
    if addresses.iter().any(|address| is_blocked(*address)) {
        Err(BlockedAddress)
    } else {
        Ok(addresses)
    }
}

#[cfg(test)]
#[path = "resolve/tests.rs"]
mod tests;
