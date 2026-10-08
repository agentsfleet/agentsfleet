//! What a sandbox's network reaches: the egress policy the daemon assigned
//! this runner, made concrete for one lease.
//!
//! `allow_all` shares the host's network namespace. `deny_all_egress`, and the
//! fail-closed answer to a missing assignment, gives the sandbox a namespace
//! of its own with nothing but loopback. `allow_list_egress` gives it that
//! namespace joined to the host by one veth pair, whose host-side rules admit
//! only the addresses of an [`Allowlist`] (`crate::egress`, Linux). The
//! sandbox resolves no name itself: the allowlist's names reach it as a
//! rendered `/etc/hosts`, and its `/etc/resolv.conf` names no server.

use std::collections::{HashMap, HashSet};
use std::iter;
use std::net::Ipv4Addr;

use crate::error::{EgressRefusal, Result, egress_refused};

/// The most addresses one lease's allowlist may hold: the host-side set is
/// built in one netlink transaction, and a list this long is a mistake, not
/// a fleet's needs.
pub const ALLOWLIST_ADDRESSES_MAX: usize = 256;
/// Where the rendered names file is bound inside the sandbox.
pub const SANDBOX_HOSTS: &str = "/etc/hosts";
/// Where the resolver-less resolver file is bound inside the sandbox.
pub const SANDBOX_RESOLV_CONF: &str = "/etc/resolv.conf";
/// The sandbox's resolver file: no server, so a name outside the allowlist
/// fails at once and no query leaves to tunnel data through.
pub const RESOLV_CONF: &str =
    "# agentsfleet: names resolve through /etc/hosts alone; no resolver.\n";
/// The loopback names every hosts file starts with: a binary that resolves
/// `localhost` through `/etc/hosts` alone still finds it.
const HOSTS_PREAMBLE: &str = "127.0.0.1 localhost\n::1 localhost\n";

/// What one lease's sandbox can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network<'a> {
    /// The host's own network, shared.
    Host,
    /// Nothing beyond loopback.
    Isolated,
    /// The addresses of the allowlist, through rules on the host.
    Allowed(&'a Allowlist),
}

/// One lease's egress allowlist, resolved at bind: each name and the IPv4
/// addresses it resolved to, names in the order they were merged.
///
/// Each name's addresses are kept sorted and once each, so two resolutions
/// that answered the same addresses in a rotated order are the same
/// allowlist: a resolver's round-robin never makes a held sandbox look built
/// for another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowlist {
    entries: Vec<(String, Ipv4Addr)>,
}

impl Allowlist {
    /// The allowlist of `entries`: each a name and one address it resolved
    /// to. A name with several addresses appears once per address.
    ///
    /// # Errors
    /// More distinct addresses than [`ALLOWLIST_ADDRESSES_MAX`]: the lease is
    /// refused before any rule is installed.
    pub fn new(mut entries: Vec<(String, Ipv4Addr)>) -> Result<Self> {
        let mut first_seen: HashMap<String, usize> = HashMap::new();
        for (name, _address) in &entries {
            let next = first_seen.len();
            first_seen.entry(name.clone()).or_insert(next);
        }
        entries.sort_by_key(|(name, address)| (first_seen.get(name).copied(), *address));
        entries.dedup();
        let allowlist = Self { entries };
        let addresses = allowlist.addresses().len();
        if addresses > ALLOWLIST_ADDRESSES_MAX {
            return Err(egress_refused(EgressRefusal::TooManyAddresses(addresses)));
        }
        Ok(allowlist)
    }

    /// How many names it admits, each counted once however many addresses
    /// it resolved to: what a log line may say of it.
    #[must_use]
    pub fn hosts(&self) -> usize {
        let mut seen = HashSet::with_capacity(self.entries.len());
        self.entries
            .iter()
            .filter(|(name, _address)| seen.insert(name.as_str()))
            .count()
    }

    /// Every distinct address, in first-seen order: the host-side set.
    #[must_use]
    pub fn addresses(&self) -> Vec<Ipv4Addr> {
        let mut seen = HashSet::with_capacity(self.entries.len());
        self.entries
            .iter()
            .map(|(_name, address)| *address)
            .filter(|address| seen.insert(*address))
            .collect()
    }

    /// The sandbox's `/etc/hosts`: loopback, then one `address name` line per
    /// entry.
    #[must_use]
    pub fn hosts_file(&self) -> String {
        iter::once(HOSTS_PREAMBLE.to_owned())
            .chain(
                self.entries
                    .iter()
                    .map(|(name, address)| format!("{address} {name}\n")),
            )
            .collect()
    }
}

#[cfg(test)]
#[path = "network/tests.rs"]
mod tests;
