//! What a lease's sandbox reaches: the egress the daemon assigned this
//! runner, made concrete for one lease when it binds.
//!
//! `allow_all` shares the host's network and `deny_all_egress` reaches
//! nothing. `allow_list_egress` reaches the operator's registry baseline, or
//! [`DEFAULT_REGISTRY`] when the operator named none, plus the hosts the
//! fleet's `network.allow` names. Each host is resolved to its IPv4 addresses
//! here, with the host's resolver, before the sandbox is built: the sandbox
//! resolves nothing itself. A host the fleet names may not resolve to an
//! address `afd_core::net` blocks (loopback, private, the tailnet's shared
//! range, link-local and the cloud metadata service, reserved), the
//! predicate `http_request` and the daemon's endpoint check refuse by too; a
//! registry host is the operator's to point at its own mirror, so it is not
//! held to it. A fleet that sets `read_only` keeps its hosts out
//! of the kernel set, because a rule on an address cannot hold a method; it
//! reaches them through `http_request` alone. The inference endpoint is never
//! in the set: models are called from the supervisor, outside every sandbox.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use afd_core::net::is_blocked;
use afd_wire::policy::NetworkPolicy as FleetNetwork;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy};
use afr_egress::allowlist_host;
use afr_sandbox::{Allowlist, Network};
use futures_util::future::try_join_all;

use crate::error::{self, Result};

/// The package registries an `allow_list_egress` sandbox reaches when its
/// operator named no registry baseline: npm, the Python Package Index,
/// crates.io and the Go module proxy, each by every host its client fetches
/// from.
pub(crate) const DEFAULT_REGISTRY: [&str; 8] = [
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "static.crates.io",
    "crates.io",
    "index.crates.io",
    "proxy.golang.org",
    "sum.golang.org",
];
/// The port a lookup is made for: any, since only the address is kept.
const ANY_PORT: u16 = 0;

/// The egress the daemon assigned this runner: its posture, and the registry
/// hosts an allowlist starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Egress {
    policy: NetworkPolicy,
    /// The registry hosts every `allow_list_egress` lease reaches: the
    /// operator's, or [`DEFAULT_REGISTRY`] when the operator named none.
    registry: Arc<[Box<str>]>,
}

impl Egress {
    /// What a runner with no readable assignment reaches: nothing.
    pub(crate) fn closed() -> Self {
        Self {
            policy: NetworkPolicy::DenyAllEgress,
            registry: Arc::from([]),
        }
    }

    /// The egress `assigned` names, each registry entry cut to its host, or
    /// [`DEFAULT_REGISTRY`] when it names none.
    pub(crate) fn assigned(assigned: &AssignedPolicy<'_>) -> Self {
        let named = &assigned.registry_allowlist;
        let registry = if named.is_empty() {
            DEFAULT_REGISTRY.map(Box::from).into()
        } else {
            named
                .iter()
                .map(|entry| Box::from(host_of(entry)))
                .collect()
        };
        Self {
            policy: assigned.network_policy,
            registry,
        }
    }

    /// The hosts an `allow_list_egress` lease under `fleet` reaches, each with
    /// who named it: the registry, then the fleet's own unless it is
    /// read-only, each cut to its host and named once, in the order first
    /// named. A host named by both is the registry's.
    pub(crate) fn hosts(&self, fleet: &FleetNetwork<'_>) -> Vec<(String, Named)> {
        let fleet_hosts = if fleet.read_only {
            &[][..]
        } else {
            fleet.allow.as_slice()
        };
        let registry = self
            .registry
            .iter()
            .map(|host| (host.to_string(), Named::Registry));
        let fleet_named = fleet_hosts
            .iter()
            .map(|entry| (host_of(entry).into_owned(), Named::Fleet));
        let mut seen = HashSet::new();
        registry
            .chain(fleet_named)
            .filter(|(host, _named)| seen.insert(host.clone()))
            .collect()
    }

    /// What a lease under `fleet` reaches, resolved through `resolver`.
    ///
    /// # Errors
    /// A host that does not resolve, or resolves to no IPv4 address, a host
    /// the fleet names that resolves to a blocked address, and an allowlist
    /// the sandbox engine will not take: each refuses the lease before its
    /// sandbox is built.
    pub(crate) async fn bind(
        &self,
        fleet: &FleetNetwork<'_>,
        resolver: &dyn Resolve,
    ) -> Result<Bound> {
        match self.policy {
            NetworkPolicy::AllowAll => Ok(Bound::Host),
            NetworkPolicy::DenyAllEgress => Ok(Bound::Isolated),
            NetworkPolicy::AllowListEgress => {
                let hosts = self.hosts(fleet);
                let addresses = try_join_all(
                    hosts
                        .iter()
                        .map(|(host, named)| ipv4(resolver, host, *named)),
                )
                .await?;
                Allowlist::new(addresses.into_iter().flatten().collect())
                    .map(Bound::Allowed)
                    .map_err(error::egress)
            }
        }
    }
}

/// One lease's egress, resolved: what its sandbox is built to reach, and what
/// a held sandbox must have been built to reach to serve it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Bound {
    /// The host's own network.
    Host,
    /// Nothing beyond loopback.
    Isolated,
    /// The allowlist's addresses.
    Allowed(Allowlist),
}

impl Bound {
    /// What it reaches by name, its addresses left out.
    pub(crate) fn reach(&self) -> Reach {
        match self {
            Self::Host => Reach::Host,
            Self::Isolated => Reach::Isolated,
            Self::Allowed(allowlist) => Reach::Allowed(allowlist.names().to_vec()),
        }
    }

    /// The network a sandbox is asked for.
    pub(crate) const fn network(&self) -> Network<'_> {
        match self {
            Self::Host => Network::Host,
            Self::Isolated => Network::Isolated,
            Self::Allowed(allowlist) => Network::Allowed(allowlist),
        }
    }
}

/// What a sandbox was built to reach, by name: what a held sandbox is filed
/// under, so a lease whose hosts resolved to new addresses still finds it and
/// the sandbox takes the new addresses in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The host's own network.
    Host,
    /// Nothing beyond loopback.
    Isolated,
    /// These names, each once, in the order merged.
    Allowed(Vec<String>),
}

/// Resolves a host name to its addresses.
#[async_trait::async_trait]
pub(crate) trait Resolve: Send + Sync + fmt::Debug {
    /// Every address `host` resolves to.
    ///
    /// # Errors
    /// The resolver could not answer for `host`.
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>>;
}

/// The host's own resolver, as every other program on it resolves.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemResolver;

#[async_trait::async_trait]
impl Resolve for SystemResolver {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let found = tokio::net::lookup_host((host, ANY_PORT)).await?;
        Ok(found.map(|socket| socket.ip()).collect())
    }
}

/// Who named a host: the operator's registry, or the fleet. A host named by
/// both is the registry's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Named {
    Registry,
    Fleet,
}

/// `host` with each IPv4 address it resolves to. A host the fleet named is
/// refused whole when any address it resolves to is blocked, IPv6 included,
/// as `http_request` refuses it: one blocked answer means the name points
/// inside.
async fn ipv4(resolver: &dyn Resolve, host: &str, named: Named) -> Result<Vec<(String, Ipv4Addr)>> {
    let addresses = resolver
        .resolve(host)
        .await
        .map_err(error::egress_unresolved(host))?;
    if named == Named::Fleet && addresses.iter().any(|address| is_blocked(*address)) {
        return Err(error::egress_blocked(host));
    }
    let admitted: Vec<_> = addresses
        .into_iter()
        .filter_map(|address| match address {
            IpAddr::V4(v4) => Some((host.to_owned(), v4)),
            IpAddr::V6(_) => None,
        })
        .collect();
    if admitted.is_empty() {
        return Err(error::egress_no_ipv4(host));
    }
    Ok(admitted)
}

/// The host an allowlist entry names, read as `http_request` reads it
/// ([`allowlist_host`]): a port, scheme or path dropped, an address literal
/// without its brackets. The kernel set admits addresses alone, so only the
/// host is resolved, and `[::1]:80` is resolved, and judged, as the address
/// `::1`. An entry no host can be read from is resolved as written, and
/// refuses the lease naming it.
fn host_of(entry: &str) -> Cow<'_, str> {
    allowlist_host(entry).map_or(Cow::Borrowed(entry), Cow::Owned)
}

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;
