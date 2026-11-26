//! The egress scope: what joins an `allow_list_egress` sandbox's network
//! namespace to the host, and holds it to its allowlist.
//!
//! Each scope takes a slot ([`slot`]). In the host's namespace it owns an
//! `nf_tables` table ([`rules`]) and the host end of a veth pair ([`link`]); the
//! pair's other end is in the sandbox's namespace, addressed and routed
//! through the host end. Every rule lives on the host, where the sandbox,
//! whose processes hold no capability, cannot reach it. Everything goes over
//! netlink from this process ([`netlink`]); no `nft` or `ip` program runs.
//!
//! ```text
//!   sandbox namespace            host namespace
//!   ┌──────────────────┐         ┌───────────────────────────────────────┐
//!   │ afpN 10.69.N.2/30 ├─veth────┤ afvN 10.69.N.1/30                     │
//!   │ default via .1   │         │ table inet afegressN: set @allow,     │
//!   │ /etc/hosts       │         │ forward / input / postrouting chains  │
//!   └──────────────────┘         └───────────────────────────────────────┘
//! ```

use std::fs::{self, File};
use std::net::Ipv4Addr;
use std::os::fd::{AsFd as _, BorrowedFd, OwnedFd};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use af_core::error_code::{Coded as _, Logged};
use afr_engine::{Allowlist, EgressRefusal};

use self::slot::{Claim, LINK_PREFIX, Slot, TABLE_PREFIX};
use crate::error::{Result, Step, egress_refused};

#[cfg(feature = "test-util")]
pub mod far;
mod guest;
mod kernel;
mod link;
mod lock;
mod netlink;
mod rules;
mod scope;
mod slot;

pub use self::guest::{
    GUEST_ADDRESS, GUEST_PREFIX_LEN, GuestNamespace, TAP, TAP_ADDRESS, remove_namespace,
};
use self::kernel::{Host, Kernel};
pub use self::lock::own_host;
use self::rules::Policy;
use self::scope::Scope;

/// Whether the host forwards IPv4 between its links: an allowlisted sandbox's
/// traffic is forwarded from its link to the host's.
const IP_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";
/// What [`IP_FORWARD`] reads when forwarding is on.
const FORWARDING: &str = "1";
/// This process's network namespace.
const OWN_NAMESPACE: &str = "/proc/self/ns/net";
/// The events the probe and the sweep are logged under.
const EVENT_PROBE_FAILED: &str = "egress_probe_failed";
const EVENT_SWEPT: &str = "egress_swept";
const EVENT_SWEEP_FAILED: &str = "egress_sweep_failed";

/// Whether this host can hold a sandbox to an allowlist: it forwards IPv4, no
/// forward chain of its own drops by policy, and a whole scope builds and
/// comes down again.
///
/// The scope is built in a namespace made for the probe, never the host's, so
/// a probe run beside a live runner neither meets its scopes nor removes them.
/// The host's chains are only read.
#[must_use]
pub fn enforceable() -> bool {
    match probe(&Host, Path::new(IP_FORWARD)) {
        Ok(()) => true,
        Err(error) => {
            let Logged { error_code, reason } = error.logged();
            let refusal = error.egress_refusal().map(EgressRefusal::as_str);
            let event = EVENT_PROBE_FAILED;
            {
                let (message,) = (format_args!(
                    "this host cannot hold a sandbox to an allowlist"
                ),);
                tracing::warn!(message, error_code, refusal, reason, event);
            };
            false
        }
    }
}

/// Whether the file at `path` reads as forwarding IPv4.
fn forwards(path: &Path) -> std::io::Result<bool> {
    Ok(fs::read_to_string(path)?.trim() == FORWARDING)
}

/// [`enforceable`] against `kernel`, reading forwarding from `forwarding`.
fn probe(kernel: &impl Kernel, forwarding: &Path) -> Result<()> {
    if !forwards(forwarding)? {
        return Err(egress_refused(EgressRefusal::ForwardingOff));
    }
    let dropping = kernel.over_netfilter(Step::ListChains, rules::dropping_forward)?;
    if !dropping.is_empty() {
        return Err(egress_refused(EgressRefusal::ForwardDropped(dropping)));
    }
    let host = kernel.fresh_namespace()?;
    let sandbox = kernel.fresh_namespace()?;
    let empty = Allowlist::new(Vec::new())?;
    kernel.inside(host.as_fd(), || {
        let scope = Scope::build(kernel, sandbox.as_fd(), &empty)?;
        scope.remove(kernel)
    })
}

/// The network namespace of a process in the sandbox whose cgroup lists its
/// processes at `procs`.
///
/// The first one whose namespace is not this process's, held open so the
/// sandbox can be joined to the host through it.
///
/// # Errors
/// `procs` is unreadable, or no process listed runs in a namespace of its own.
pub fn namespace_of(procs: &Path) -> Result<OwnedFd> {
    let own = fs::metadata(OWN_NAMESPACE)?;
    let listed = fs::read_to_string(procs)?;
    listed
        .split_whitespace()
        .find_map(|pid| {
            let file = File::open(format!("/proc/{pid}/ns/net")).ok()?;
            // Read from the open file, not the path, so a pid reused between
            // the two is never mistaken for the sandbox's.
            let found = file.metadata().ok()?;
            (found.ino() != own.ino() || found.dev() != own.dev()).then(|| OwnedFd::from(file))
        })
        .ok_or_else(|| egress_refused(EgressRefusal::NoNamespace))
}

/// Removes every egress table and link a previous run of this host left, in
/// the host's namespace, skipping any slot a scope in this process holds.
///
/// # Errors
/// The kernel would not list the tables or the links; each object it lists
/// but will not remove is logged and left.
pub fn sweep() -> Result<()> {
    sweep_from(&Host)
}

/// One sandbox's scope on the running host: its slot, the host-side table
/// holding it to its allowlist, and the veth pair joining its namespace to
/// the host. [`HostScope::remove`] is its one release.
#[derive(Debug)]
pub struct HostScope(Scope);

/// What a guarded scope lets its sandbox reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destinations<'a> {
    /// The allowlist's addresses, the resolver port closed.
    Listed(&'a Allowlist),
    /// Every public IPv4 address: every reserved range dropped before the
    /// accept.
    Public,
}

impl HostScope {
    /// Joins `netns` to the host through a slot of its own, admitting only
    /// `allowlist`'s addresses.
    ///
    /// # Errors
    /// No slot is free, or the kernel refused a step; what the build made is
    /// removed before the refusal returns.
    pub fn join(netns: BorrowedFd<'_>, allowlist: &Allowlist) -> Result<Self> {
        Scope::build(&Host, netns, allowlist).map(Self)
    }

    /// Joins `netns` to the host through a slot of its own, admitting
    /// `destinations`: a listed plan its set alone, as a bubblewrap scope
    /// does, so an operator registry at a private address stays reachable;
    /// a public plan every address once each reserved range — private,
    /// loopback, link-local and carrier-grade among them — is dropped.
    ///
    /// # Errors
    /// As [`HostScope::join`].
    pub fn guarded(netns: BorrowedFd<'_>, destinations: Destinations<'_>) -> Result<Self> {
        let empty;
        let (allowlist, reach) = match destinations {
            Destinations::Listed(allowlist) => (allowlist, rules::Reach::Listed),
            Destinations::Public => {
                empty = Allowlist::new(Vec::new())?;
                (&empty, rules::Reach::Public)
            }
        };
        let policy = Policy {
            reach,
            reserved_dropped: reach == rules::Reach::Public,
        };
        Scope::build_held(&Host, netns, allowlist, policy).map(Self)
    }

    /// Holds the scope to `allowlist`'s addresses from now on, in place of
    /// the ones it was built with.
    ///
    /// # Errors
    /// The kernel refused the swap; the set holds what it held before.
    pub fn refill(&mut self, allowlist: &Allowlist) -> Result<()> {
        self.0.refill(allowlist)
    }

    /// The addresses the kernel holds this scope to, read back.
    ///
    /// # Errors
    /// The kernel refused the socket or the read.
    pub fn enforced(&self) -> Result<Vec<Ipv4Addr>> {
        self.0.enforced(&Host)
    }

    /// Deletes the link, then the table, and frees the slot.
    ///
    /// # Errors
    /// The kernel refused a removal; the slot stays held for this process's
    /// life.
    pub fn remove(self) -> Result<()> {
        self.0.remove(&Host)
    }
}

/// [`sweep`] through `kernel`.
fn sweep_from(kernel: &impl Kernel) -> Result<()> {
    let (tables, links) = objects(kernel)?;
    let mut slots: Vec<Slot> = tables
        .iter()
        .filter_map(|table| Slot::named(table, TABLE_PREFIX))
        .chain(
            links
                .iter()
                .filter_map(|link| Slot::named(link, LINK_PREFIX)),
        )
        .collect();
    slots.sort_unstable_by_key(|slot| slot.index());
    slots.dedup();
    let pool = kernel.slots();
    for claim in slots
        .into_iter()
        .filter_map(|slot| Claim::exactly(pool, slot))
    {
        let slot = claim.slot().index();
        match scope::remove(kernel, claim.slot()) {
            Ok(()) => {
                let event = EVENT_SWEPT;
                tracing::info!(slot, event);
            }
            Err(error) => {
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_SWEEP_FAILED;
                tracing::warn!(slot, error_code, reason, event);
                claim.abandon();
            }
        }
    }
    Ok(())
}

/// Every egress table and every egress link in the calling thread's
/// namespace, by name.
///
/// # Errors
/// The kernel would not list them.
fn objects(kernel: &impl Kernel) -> Result<(Vec<String>, Vec<String>)> {
    let tables = kernel.over_netfilter(Step::ListTables, |netfilter| {
        rules::names(netfilter, TABLE_PREFIX)
    })?;
    let links = kernel.over_route(Step::ListLinks, |route| link::names(route, LINK_PREFIX))?;
    Ok((tables, links))
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod tests;
