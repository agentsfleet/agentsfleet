//! One sandbox's scope: its slot, the host-side table holding it to its
//! allowlist, and the veth pair joining its namespace to the host.

use std::os::fd::BorrowedFd;

use super::kernel::Kernel;
use super::netlink::Netlink;
use super::slot::{Claim, Slot};
use super::{link, rules};
use crate::error::{Error, Result, egress_refused, netlink};
use crate::network::Allowlist;

/// Why a scope could not claim a slot.
const NO_SLOT: &str = "every egress slot on this host is held by a running sandbox";
/// What each netlink step is named in a refusal.
const OPEN_NETFILTER: &str = "a netfilter socket";
const INSTALL_RULES: &str = "the egress table";
const OPEN_ROUTE: &str = "a route socket";
const JOIN: &str = "the veth pair joining the sandbox to the host";
const CONFIGURE_PEER: &str = "the sandbox side of its veth pair";
const REMOVE_RULES: &str = "removing the egress table";
const REMOVE_LINK: &str = "removing the veth pair";
/// The events a scope's life is logged under.
const EVENT_BUILT: &str = "egress_scope_built";
const EVENT_REFUSED: &str = "egress_scope_refused";
const EVENT_LEFT: &str = "egress_scope_left";

/// A built scope; [`Scope::remove`] is its one release.
#[derive(Debug)]
pub(crate) struct Scope {
    claim: Claim,
}

impl Scope {
    /// Joins `netns` to the host through a slot of its own, admitting only
    /// `allowlist`'s addresses. The table comes first, so the link never
    /// carries a packet its rules have not seen.
    ///
    /// # Errors
    /// No slot is free, or the kernel refused a step. What the build made is
    /// removed before the refusal returns; what cannot be removed keeps its
    /// slot held, for the next run's boot sweep.
    pub(crate) fn build(
        kernel: &impl Kernel,
        netns: BorrowedFd<'_>,
        allowlist: &Allowlist,
    ) -> Result<Self> {
        let hosts = allowlist.hosts();
        let Some(claim) = Claim::any() else {
            let error = egress_refused(NO_SLOT);
            refused(None, hosts, &error);
            return Err(error);
        };
        let slot = claim.slot();
        // Nothing reaches the kernel before this socket opens, so a host that
        // will not give one has nothing to undo: the claim drops and frees the
        // slot, where a later failure keeps it until the removal is confirmed.
        let mut netfilter = match kernel.netfilter().map_err(netlink(OPEN_NETFILTER)) {
            Ok(netfilter) => netfilter,
            Err(error) => {
                refused(Some(slot), hosts, &error);
                return Err(error);
            }
        };
        let scope = Self { claim };
        match scope.attach(kernel, &mut netfilter, netns, allowlist) {
            Ok(()) => {
                let slot = slot.index();
                let event = EVENT_BUILT;
                tracing::info!(slot, hosts, event);
                Ok(scope)
            }
            Err(error) => {
                refused(Some(slot), hosts, &error);
                // Logged where it fails; the refusal is the build's error.
                let _left = scope.remove(kernel);
                Err(error)
            }
        }
    }

    fn attach<K: Kernel>(
        &self,
        kernel: &K,
        netfilter: &mut Netlink<K::Wire>,
        netns: BorrowedFd<'_>,
        allowlist: &Allowlist,
    ) -> Result<()> {
        let slot = self.claim.slot();
        rules::apply(netfilter, slot, &allowlist.addresses()).map_err(netlink(INSTALL_RULES))?;
        let mut route = kernel.route().map_err(netlink(OPEN_ROUTE))?;
        link::join(&mut route, slot, netns).map_err(netlink(JOIN))?;
        kernel
            .inside(netns, || link::configure_peer(&mut kernel.route()?, slot))
            .map_err(netlink(CONFIGURE_PEER))
    }

    /// Deletes the link, then the table, and frees the slot; anything already
    /// gone is not a failure.
    ///
    /// # Errors
    /// The kernel refused a removal. The slot stays held for this process's
    /// life, so no later scope meets what was left.
    pub(crate) fn remove(self, kernel: &impl Kernel) -> Result<()> {
        let slot = self.claim.slot();
        match remove(kernel, slot) {
            Ok(()) => Ok(()),
            Err(error) => {
                let error_code = error.code().as_str();
                let reason = error.told();
                let event = EVENT_LEFT;
                tracing::warn!(slot = slot.index(), error_code, reason, event);
                self.claim.abandon();
                Err(error)
            }
        }
    }
}

/// Removes whatever `slot` holds on the host: its link, then its table.
pub(super) fn remove(kernel: &impl Kernel, slot: Slot) -> Result<()> {
    let link = kernel
        .route()
        .and_then(|mut route| link::remove(&mut route, &slot.link()))
        .map_err(netlink(REMOVE_LINK));
    let table = kernel
        .netfilter()
        .and_then(|mut netfilter| rules::remove(&mut netfilter, &slot.table()))
        .map_err(netlink(REMOVE_RULES));
    link.and(table)
}

/// Logs a scope that could not be built: why, its slot when it had one, and
/// how many names it would have admitted — never the names or addresses.
fn refused(slot: Option<Slot>, hosts: usize, error: &Error) {
    let slot = slot.map(Slot::index);
    let error_code = error.code().as_str();
    let reason = error.told();
    let event = EVENT_REFUSED;
    tracing::warn!(slot, hosts, error_code, reason, event);
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
