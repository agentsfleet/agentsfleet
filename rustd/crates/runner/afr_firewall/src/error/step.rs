//! Which netlink step of an egress scope's life the kernel refused: one value
//! each, so a log line and a test tell them apart while the sentence an
//! operator reads stays the one each has always been.

use std::fmt;

/// The netlink step of an egress scope's life the kernel refused.
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
    /// Replacing the addresses in the scope's set.
    RefillRules,
    /// Reading back the addresses the scope's set holds.
    ReadRules,
    /// Removing the veth pair.
    RemoveLink,
    /// Listing the host's forward chains.
    ListChains,
    /// Listing egress tables.
    ListTables,
    /// Listing egress links.
    ListLinks,
    /// Making, addressing or raising a guest's tap in its namespace.
    Tap,
    /// Building the far host the kernel lane connects from.
    #[cfg(feature = "test-util")]
    Far,
}

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
            Self::RefillRules => "refilling the egress set",
            Self::ReadRules => "reading back the egress set",
            Self::RemoveLink => "removing the veth pair",
            Self::ListChains => "listing the host's forward chains",
            Self::ListTables => "listing egress tables",
            Self::ListLinks => "listing egress links",
            Self::Tap => "the guest's tap in its namespace",
            #[cfg(feature = "test-util")]
            Self::Far => "the far host's link",
        })
    }
}
