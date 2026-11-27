//! The `nf_tables` table that holds one slot to its allowlist, in the host's
//! namespace: the messages, pure, and the conversations made of them.
//!
//! One table per slot, `inet afegress<slot>`. Every base chain accepts by
//! policy and every drop names the slot's link, so one table never touches
//! another sandbox's traffic or the host's own:
//!
//! ```text
//! forward      iif afvN ip saddr != 10.69.N.2 drop
//!              iif afvN tcp dport 53 drop · iif afvN udp dport 53 drop
//!              iif afvN ip daddr @allow accept · iif afvN drop
//!              oif afvN ct state established,related accept · oif afvN drop
//! input        iif afvN drop
//! postrouting  ip saddr 10.69.N.0/30 oif != afvN masquerade
//! ```
//!
//! The table is built in one batch, which the kernel applies whole or not at
//! all, so a slot never has half its rules.
//!
//! The table is owned by the socket that built it (`NFT_TABLE_F_OWNER`): no
//! other socket may change or delete it, a `nft flush ruleset` on the host
//! passes it by, and the kernel removes it when that socket closes, a crashed
//! runner's included. [`super::Scope`] keeps the socket for the scope's life.

use std::io;
use std::net::Ipv4Addr;

use netlink_packet_core::NetlinkMessage;
#[cfg(feature = "test-util")]
use netlink_packet_netfilter::NetfilterProtoFamily;
use netlink_packet_netfilter::nftables::{
    InetHookNumber, NfTablesMessage, TableAttribute, TableFlags, TableMessage,
};
use netlink_packet_netfilter::{NetfilterMessage, NetfilterMessageInner};

use self::expressions::{ACCEPT, forward, from, from_another, masquerade};
#[cfg(feature = "test-util")]
use self::message::nf_in;
use self::message::{base_chain, batch, new_rule, new_table, nf};
use super::netlink::{Netlink, Wire};
use super::slot::{PREFIX_LEN, Slot};

mod expressions;
mod host_chains;
mod message;
mod set;

#[cfg(feature = "test-util")]
pub(super) use self::expressions::DNS_PORT;
pub(super) use self::expressions::DROP;
pub(super) use self::host_chains::dropping_forward;
#[cfg(test)]
pub(super) use self::message::u16_of;
pub(super) use self::message::{u8_of, u32_of};
use self::set::{elements, new_set};
pub(super) use self::set::{enforced, refill};

/// One `nf_tables` message.
pub(super) type Message = NetlinkMessage<NetfilterMessage>;

/// The set of addresses the slot may reach.
pub(super) const SET: &str = "allow";
/// The set's number within the batch that makes it, so the rules made in the
/// same batch find it.
pub(super) const SET_ID: u32 = 1;
/// The chains, their hooks, kinds and priorities: filtering at the filter
/// priority, address translation at source-NAT's.
const FORWARD: &str = "forward";
const INPUT: &str = "input";
const POSTROUTING: &str = "postrouting";
const FILTER: &str = "filter";
const NAT: &str = "nat";
const FILTER_PRIORITY: u32 = 0;
const SOURCE_NAT_PRIORITY: u32 = 100;
/// What a slot's sandbox reaches past its link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The addresses in its set, the resolver port closed.
    Listed,
    /// Every IPv4 destination.
    Public,
}

/// How a slot's table holds its sandbox: what it reaches, and whether every
/// reserved range is dropped before anything is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Policy {
    pub(crate) reach: Reach,
    pub(crate) reserved_dropped: bool,
}

impl Policy {
    /// The set alone, as a shared-kernel sandbox is held: an operator's
    /// registry may resolve to a private address, and stays admitted.
    pub(crate) const LISTED: Self = Self {
        reach: Reach::Listed,
        reserved_dropped: false,
    };
}

/// Every IPv4 range `af_core::net::is_blocked` refuses, as a network and its
/// prefix length: what a guarded slot drops before any accept, so no plan
/// reaches the host's private networks, the metadata service, loopback or
/// carrier-grade space whatever its set holds.
pub(crate) const RESERVED: [(Ipv4Addr, u8); 9] = [
    (Ipv4Addr::UNSPECIFIED, 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::LOCALHOST, 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(224, 0, 0, 0), 3),
];

/// The batch that builds `slot`'s table, set and chains, admitting `addresses`,
/// owned by the socket that sends it, as a shared-kernel sandbox's is.
#[cfg(test)]
pub(super) fn install(slot: Slot, addresses: &[Ipv4Addr]) -> Vec<Message> {
    install_flagged(slot, addresses, TableFlags::Owner, Policy::LISTED)
}

/// The batch that builds `slot`'s table, set and chains under `policy`,
/// admitting `addresses`, owned by the socket that sends it.
pub(super) fn install_held(slot: Slot, addresses: &[Ipv4Addr], policy: Policy) -> Vec<Message> {
    install_flagged(slot, addresses, TableFlags::Owner, policy)
}

/// [`install`] with no owner, so the table outlives the socket that sent it:
/// the leftover a runner built before tables were owned leaves behind.
#[cfg(feature = "test-util")]
pub(super) fn install_unowned(slot: Slot, addresses: &[Ipv4Addr]) -> Vec<Message> {
    install_flagged(slot, addresses, TableFlags::empty(), Policy::LISTED)
}

fn install_flagged(
    slot: Slot,
    addresses: &[Ipv4Addr],
    flags: TableFlags,
    policy: Policy,
) -> Vec<Message> {
    let table = slot.table();
    let link = slot.link();
    let mut messages = vec![new_table(&table, flags), new_set(&table)];
    messages.extend(slot_chains(&table));
    if !addresses.is_empty() {
        messages.push(elements(&table, addresses));
    }
    // First in the chain: no accept below sees a source the slot was not given.
    let own_source = std::iter::once(from_another(&link, slot.sandbox()));
    messages.extend(
        own_source
            .chain(forward(&link, policy))
            .map(|rule| new_rule(&table, FORWARD, rule)),
    );
    messages.push(new_rule(&table, INPUT, from(&link, Vec::new(), DROP)));
    messages.push(new_rule(
        &table,
        POSTROUTING,
        masquerade(slot.network(), PREFIX_LEN, &link),
    ));
    batch(messages)
}

/// A slot's three base chains, each accepting by policy: forward and input at
/// the filter priority, postrouting at source-NAT's.
fn slot_chains(table: &str) -> [Message; 3] {
    [
        base_chain(
            table,
            FORWARD,
            FILTER,
            InetHookNumber::Forward,
            FILTER_PRIORITY,
            ACCEPT,
        ),
        base_chain(
            table,
            INPUT,
            FILTER,
            InetHookNumber::LocalIn,
            FILTER_PRIORITY,
            ACCEPT,
        ),
        base_chain(
            table,
            POSTROUTING,
            NAT,
            InetHookNumber::PostRouting,
            SOURCE_NAT_PRIORITY,
            ACCEPT,
        ),
    ]
}

/// The batch that builds the table a guest's namespace forwards through,
/// owned by no socket so it lives as long as the namespace: the tap forwards
/// only what `guest`, the one address on it, sends; that leaves under the
/// namespace's own address, as everything of the tap's `network`/`prefix_len`
/// does; and nothing from the tap reaches the namespace itself.
pub(super) fn install_guest_side(
    table: &str,
    tap: &str,
    guest: Ipv4Addr,
    (network, prefix_len): (Ipv4Addr, u8),
) -> Vec<Message> {
    batch(vec![
        new_table(table, TableFlags::empty()),
        base_chain(
            table,
            FORWARD,
            FILTER,
            InetHookNumber::Forward,
            FILTER_PRIORITY,
            ACCEPT,
        ),
        base_chain(
            table,
            INPUT,
            FILTER,
            InetHookNumber::LocalIn,
            FILTER_PRIORITY,
            ACCEPT,
        ),
        base_chain(
            table,
            POSTROUTING,
            NAT,
            InetHookNumber::PostRouting,
            SOURCE_NAT_PRIORITY,
            ACCEPT,
        ),
        new_rule(table, FORWARD, from_another(tap, guest)),
        new_rule(table, INPUT, from(tap, Vec::new(), DROP)),
        new_rule(table, POSTROUTING, masquerade(network, prefix_len, tap)),
    ])
}

/// The batch that deletes the table named `table`, its set and chains with it.
pub(super) fn uninstall(table: &str) -> Vec<Message> {
    batch(vec![nf(
        NfTablesMessage::DeleteTable(TableMessage {
            attributes: vec![TableAttribute::Name(table.to_owned())],
        }),
        0,
    )])
}

/// The batch `nft flush ruleset` sends, narrowed to tables named `table`: a
/// deletion addressed to no family, which the kernel answers by flushing every
/// matching table the sending socket may touch.
#[cfg(feature = "test-util")]
pub(super) fn flush_named(table: &str) -> Vec<Message> {
    let inner = NfTablesMessage::DeleteTable(TableMessage {
        attributes: vec![TableAttribute::Name(table.to_owned())],
    });
    batch(vec![nf_in(NetfilterProtoFamily::Unspec, inner, 0)])
}

/// A table named `table`, owned by nobody, holding one forward base chain
/// whose policy drops, as ufw or Docker leave on a host.
#[cfg(feature = "test-util")]
pub(super) fn dropping_forward_table(table: &str) -> Vec<Message> {
    batch(vec![
        new_table(table, TableFlags::empty()),
        base_chain(
            table,
            FORWARD,
            FILTER,
            InetHookNumber::Forward,
            FILTER_PRIORITY,
            DROP,
        ),
    ])
}

/// Asks for every `inet` table.
pub(super) fn every() -> Message {
    nf(
        NfTablesMessage::GetTable(TableMessage {
            attributes: Vec::new(),
        }),
        0,
    )
}

/// Builds `slot`'s table under `policy` in one transaction.
///
/// # Errors
/// The kernel refused the batch; nothing of it was applied.
pub(super) fn apply<W: Wire>(
    netfilter: &mut Netlink<W>,
    slot: Slot,
    addresses: &[Ipv4Addr],
    policy: Policy,
) -> io::Result<()> {
    netfilter.acknowledged(install_held(slot, addresses, policy))
}

/// Deletes the table named `table`; one already gone is not a failure.
///
/// # Errors
/// The kernel refused for any other reason.
pub(super) fn remove<W: Wire>(netfilter: &mut Netlink<W>, table: &str) -> io::Result<()> {
    match netfilter.acknowledged(uninstall(table)) {
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(()),
        removed => removed,
    }
}

/// The name of every `inet` table starting with `prefix`.
///
/// # Errors
/// The kernel refused the dump.
pub(super) fn names<W: Wire>(netfilter: &mut Netlink<W>, prefix: &str) -> io::Result<Vec<String>> {
    let tables = netfilter.dump(every())?;
    Ok(tables
        .into_iter()
        .filter_map(|message| match message.inner {
            NetfilterMessageInner::NfTables(NfTablesMessage::NewTable(table)) => table
                .attributes
                .into_iter()
                .find_map(|attribute| match attribute {
                    TableAttribute::Name(name) => Some(name),
                    _ => None,
                }),
            _ => None,
        })
        .filter(|name| name.starts_with(prefix))
        .collect())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod verdict_tests;
