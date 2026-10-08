//! The host's own forward chains, read before the probe reports enforcement.
//!
//! A packet crosses the forward hook only when every base chain on it accepts,
//! so a host chain whose policy drops — ufw's `DEFAULT_FORWARD_POLICY="DROP"`,
//! Docker's `FORWARD` chain — ends every connection a sandbox's table admits.
//! The probe's own scope is built in namespaces of its own and never meets
//! those chains, so they are read here, from the host's namespace.
//!
//! Only the policy is read. A dropping chain that accepts the sandbox links by
//! rule still reads as dropping: rules cannot be judged without evaluating
//! them, and the runner's host requirements already name a forward policy that
//! accepts. Chains of the legacy `iptables` backend are not visible to
//! `nf_tables` and are not read.

use std::io;

use netlink_packet_core::NetlinkMessage;
use netlink_packet_netfilter::nftables::{ChainAttribute, ChainMessage, Hook, NfTablesMessage};
use netlink_packet_netfilter::{
    NetfilterHeader, NetfilterMessage, NetfilterMessageInner, NetfilterProtoFamily,
};

use super::super::netlink::{Netlink, Wire};
use super::expressions::DROP;

/// The forward hook's number, the same in every family that carries IPv4
/// (`NF_INET_FORWARD`).
const FORWARD_HOOK: u32 = 2;
/// The two families whose forward chains see the sandbox's IPv4 traffic, as
/// `nft` names them.
const INET: &str = "inet";
const IP: &str = "ip";

/// Every base chain on the forward hook of a family carrying IPv4 whose policy
/// drops, each named `family table chain`.
///
/// # Errors
/// The kernel refused the dump.
pub(in crate::egress) fn dropping_forward<W: Wire>(
    netfilter: &mut Netlink<W>,
) -> io::Result<Vec<String>> {
    Ok(netfilter
        .dump(every_chain())?
        .into_iter()
        .filter_map(dropping)
        .collect())
}

/// Asks for every chain, of every family.
fn every_chain() -> NetlinkMessage<NetfilterMessage> {
    let header = NetfilterHeader::new(NetfilterProtoFamily::Unspec, 0, 0);
    NetlinkMessage::from(NetfilterMessage::new(
        header,
        NfTablesMessage::GetChain(ChainMessage {
            attributes: Vec::new(),
        }),
    ))
}

/// `message`'s name, when it is a forward base chain of a family carrying
/// IPv4 and its policy drops.
fn dropping(message: NetfilterMessage) -> Option<String> {
    let family = match message.header.family {
        NetfilterProtoFamily::Inet => INET,
        NetfilterProtoFamily::IPv4 => IP,
        _other => return None,
    };
    let NetfilterMessageInner::NfTables(NfTablesMessage::NewChain(chain)) = message.inner else {
        return None;
    };
    let attributes = chain.attributes;
    let forward = attributes.iter().any(|attribute| {
        matches!(attribute, ChainAttribute::Hook(hooks)
            if hooks.iter().any(|hook| matches!(hook, Hook::Number(number)
                if u32::from(*number) == FORWARD_HOOK)))
    });
    let drops = attributes
        .iter()
        .any(|attribute| matches!(attribute, ChainAttribute::Policy(policy) if *policy == DROP));
    (forward && drops).then_some(())?;
    let table = attributes.iter().find_map(|attribute| match attribute {
        ChainAttribute::Table(table) => Some(table.as_str()),
        _ => None,
    })?;
    let name = attributes.iter().find_map(|attribute| match attribute {
        ChainAttribute::Name(name) => Some(name.as_str()),
        _ => None,
    })?;
    Some(format!("{family} {table} {name}"))
}

#[cfg(test)]
#[path = "host_chains_tests.rs"]
mod tests;
