//! Why a lease's egress was refused, and which netlink step the kernel
//! refused: one value each, so a log line and a test tell them apart while
//! the sentence an operator reads stays the one each has always been.

use std::fmt;

use crate::network::ALLOWLIST_ADDRESSES_MAX;

/// What [`EgressRefusal::ForwardingOff`] says.
const FORWARDING_OFF: &str = "the host does not forward IPv4 (net.ipv4.ip_forward is not 1)";
/// What [`EgressRefusal::ForwardDropped`] says, before the chains it names.
const FORWARD_DROPPED: &str =
    "a forward chain on the host drops by policy, so no allowlisted connection would pass";
/// What [`EgressRefusal::NoNamespace`] says.
const NO_NAMESPACE: &str = "no process in the sandbox runs in a network namespace of its own";
/// What [`EgressRefusal::NoSlot`] says.
const NO_SLOT: &str = "every egress slot on this host is held by a running sandbox";
/// What [`EgressRefusal::HeldElsewhere`] says.
const HELD_ELSEWHERE: &str = "another runner process owns this host's egress tables and links";

/// Why a lease's egress cannot be held to what its policy allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressRefusal {
    /// The host does not forward IPv4, so no allowlisted packet would leave.
    ForwardingOff,
    /// Forward chains on the host drop by policy, each named
    /// `family table chain`.
    ForwardDropped(Vec<String>),
    /// No process in the sandbox runs in a network namespace of its own.
    NoNamespace,
    /// Every egress slot on the host is held.
    NoSlot,
    /// Another runner process owns the host's egress objects.
    HeldElsewhere,
    /// The allowlist resolved to this many distinct addresses, past
    /// [`ALLOWLIST_ADDRESSES_MAX`].
    TooManyAddresses(usize),
}

impl EgressRefusal {
    /// How a log line spells it.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ForwardingOff => "forwarding_off",
            Self::ForwardDropped(_) => "forward_dropped",
            Self::NoNamespace => "no_namespace",
            Self::NoSlot => "no_slot",
            Self::HeldElsewhere => "held_elsewhere",
            Self::TooManyAddresses(_) => "too_many_addresses",
        }
    }
}

impl fmt::Display for EgressRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForwardingOff => f.write_str(FORWARDING_OFF),
            Self::ForwardDropped(chains) => write!(f, "{FORWARD_DROPPED}: {}", chains.join(", ")),
            Self::NoNamespace => f.write_str(NO_NAMESPACE),
            Self::NoSlot => f.write_str(NO_SLOT),
            Self::HeldElsewhere => f.write_str(HELD_ELSEWHERE),
            Self::TooManyAddresses(addresses) => write!(
                f,
                "{addresses} addresses, past the {ALLOWLIST_ADDRESSES_MAX} a lease may reach"
            ),
        }
    }
}

/// The netlink step of an egress scope's life the kernel refused.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Opening the `nf_tables` socket a scope's table is owned by.
    OpenNetfilter,
    /// Installing the scope's table.
    InstallRules,
    /// Opening a route socket.
    OpenRoute,
    /// Creating the veth pair that joins the sandbox to the host.
    Join,
    /// Configuring the sandbox's end of the pair.
    ConfigurePeer,
    /// Removing the scope's table.
    RemoveRules,
    /// Removing the veth pair.
    RemoveLink,
    /// Listing the host's forward chains.
    ListChains,
    /// Listing egress tables.
    ListTables,
    /// Listing egress links.
    ListLinks,
    /// Building the far host the kernel lane connects from.
    #[cfg(feature = "test-util")]
    Far,
}

#[cfg(target_os = "linux")]
impl fmt::Display for Step {
    /// What was asked of the kernel, as "the kernel refused …" ends.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OpenNetfilter => "a netfilter socket",
            Self::InstallRules => "the egress table",
            Self::OpenRoute => "a route socket",
            Self::Join => "the veth pair joining the sandbox to the host",
            Self::ConfigurePeer => "the sandbox side of its veth pair",
            Self::RemoveRules => "removing the egress table",
            Self::RemoveLink => "removing the veth pair",
            Self::ListChains => "listing the host's forward chains",
            Self::ListTables => "listing egress tables",
            Self::ListLinks => "listing egress links",
            #[cfg(feature = "test-util")]
            Self::Far => "the far host's link",
        })
    }
}
