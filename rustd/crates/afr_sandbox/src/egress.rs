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
use std::os::fd::{AsFd as _, OwnedFd};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use self::kernel::Kernel;
use self::slot::{Claim, LINK_PREFIX, Slot, TABLE_PREFIX};
use crate::error::{Result, egress_refused, netlink};
use crate::network::Allowlist;

#[cfg(feature = "test-util")]
pub mod far;
mod kernel;
mod link;
mod netlink;
mod rules;
mod scope;
mod slot;

pub(crate) use self::kernel::Host;
pub(crate) use self::scope::Scope;

/// Whether the host forwards IPv4 between its links: an allowlisted sandbox's
/// traffic is forwarded from its link to the host's.
const IP_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";
/// What [`IP_FORWARD`] reads when forwarding is on.
const FORWARDING: &str = "1";
/// This process's network namespace.
const OWN_NAMESPACE: &str = "/proc/self/ns/net";
/// The probe's refusals.
const FORWARDING_OFF: &str = "the host does not forward IPv4 (net.ipv4.ip_forward is not 1)";
const NO_NAMESPACE: &str = "no process in the sandbox runs in a network namespace of its own";
/// What a sweep step is named in a refusal.
const LIST_TABLES: &str = "listing egress tables";
const LIST_LINKS: &str = "listing egress links";
/// The events the probe and the sweep are logged under.
const EVENT_PROBE_FAILED: &str = "egress_probe_failed";
const EVENT_SWEPT: &str = "egress_swept";
const EVENT_SWEEP_FAILED: &str = "egress_sweep_failed";

/// Whether this host can hold a sandbox to an allowlist: it forwards IPv4, and
/// a whole scope builds and comes down again.
///
/// The scope is built in a namespace made for the probe, never the host's, so
/// a probe run beside a live runner neither meets its scopes nor removes them.
pub(crate) fn enforceable() -> bool {
    match probe(&Host, Path::new(IP_FORWARD)) {
        Ok(()) => true,
        Err(error) => {
            let error_code = error.code().as_str();
            let reason = error.to_string();
            let event = EVENT_PROBE_FAILED;
            tracing::warn!(
                error_code,
                reason,
                event,
                "this host cannot hold a sandbox to an allowlist"
            );
            false
        }
    }
}

/// [`enforceable`] against `kernel`, reading forwarding from `forwarding`.
fn probe(kernel: &impl Kernel, forwarding: &Path) -> Result<()> {
    if fs::read_to_string(forwarding)?.trim() != FORWARDING {
        return Err(egress_refused(FORWARDING_OFF));
    }
    let host = kernel.fresh_namespace()?;
    let sandbox = kernel.fresh_namespace()?;
    let empty = Allowlist::new(Vec::new())?;
    kernel.inside(host.as_fd(), || {
        Scope::build(kernel, sandbox.as_fd(), &empty).and_then(|scope| scope.remove(kernel))
    })
}

/// The network namespace of a process in the sandbox whose cgroup lists its
/// processes at `procs`: the first one whose namespace is not this
/// process's, held open so the sandbox can be joined to the host through it.
///
/// # Errors
/// `procs` is unreadable, or no process listed runs in a namespace of its own.
pub(crate) fn namespace_of(procs: &Path) -> Result<OwnedFd> {
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
        .ok_or_else(|| egress_refused(NO_NAMESPACE))
}

/// Removes every egress table and link a previous run of this host left, in
/// the host's namespace, skipping any slot a scope in this process holds.
///
/// # Errors
/// The kernel would not list the tables or the links; each object it lists
/// but will not remove is logged and left.
pub(crate) fn sweep(kernel: &impl Kernel) -> Result<()> {
    let tables = kernel
        .netfilter()
        .and_then(|mut netfilter| rules::names(&mut netfilter, TABLE_PREFIX))
        .map_err(netlink(LIST_TABLES))?;
    let links = kernel
        .route()
        .and_then(|mut route| link::names(&mut route, LINK_PREFIX))
        .map_err(netlink(LIST_LINKS))?;
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
    for claim in slots.into_iter().filter_map(Claim::exactly) {
        let slot = claim.slot().index();
        match scope::remove(kernel, claim.slot()) {
            Ok(()) => {
                let event = EVENT_SWEPT;
                tracing::info!(slot, event);
            }
            Err(error) => {
                let error_code = error.code().as_str();
                let reason = error.to_string();
                let event = EVENT_SWEEP_FAILED;
                tracing::warn!(slot, error_code, reason, event);
                claim.abandon();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod testing;

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;
