//! The `nf_tables` table that holds one slot to its allowlist, in the host's
//! namespace: the messages, pure, and the conversations made of them.
//!
//! One table per slot, `inet afegress<slot>`. Every base chain accepts by
//! policy and every drop names the slot's link, so one table never touches
//! another sandbox's traffic or the host's own:
//!
//! ```text
//! forward      iif afvN tcp dport 53 drop · iif afvN udp dport 53 drop
//!              iif afvN ip daddr @allow accept · iif afvN drop
//!              oif afvN ct state established,related accept · oif afvN drop
//! input        iif afvN drop
//! postrouting  ip saddr 10.69.N.0/30 oif != afvN masquerade
//! ```
//!
//! The table is built in one batch, which the kernel applies whole or not at
//! all, so a slot never has half its rules.

use std::io;
use std::net::Ipv4Addr;

use netlink_packet_core::{
    NLM_F_ACK, NLM_F_APPEND, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST, NetlinkMessage,
};
use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, DataAttribute, Hook, InetHookNumber, ListAttribute,
    NfTablesMessage, RuleAttribute, RuleMessage, SetAttribute, SetElementAttribute, SetElementList,
    SetElementMessage, SetMessage, TableAttribute, TableMessage,
};
use netlink_packet_netfilter::none::ControlMessage;
use netlink_packet_netfilter::{
    NetfilterHeader, NetfilterMessage, NetfilterMessageInner, NetfilterProtoFamily,
};

use self::expressions::{ACCEPT, DROP, Expression, WORD, forward, from, masquerade};
use super::netlink::{Netlink, Wire};
use super::slot::Slot;

mod expressions;

/// One `nf_tables` message.
pub(super) type Message = NetlinkMessage<NetfilterMessage>;

/// The set of addresses the slot may reach.
pub(super) const SET: &str = "allow";
/// The set's number within the batch that makes it, so the rules made in the
/// same batch find it.
pub(super) const SET_ID: u32 = 1;
/// The kernel's own number for the `ipv4_addr` data type.
const IPV4_ADDR_TYPE: u32 = 7;
/// The netfilter subsystem a batch is addressed to (`NFNL_SUBSYS_NFTABLES`).
const NFTABLES: u16 = 10;
/// The chains, their hooks, kinds and priorities: filtering at the filter
/// priority, address translation at source-NAT's.
const FORWARD: &str = "forward";
const INPUT: &str = "input";
const POSTROUTING: &str = "postrouting";
const FILTER: &str = "filter";
const NAT: &str = "nat";
const FILTER_PRIORITY: u32 = 0;
const SOURCE_NAT_PRIORITY: u32 = 100;
/// The batch that builds `slot`'s table, set and chains, admitting `addresses`.
pub(super) fn install(slot: Slot, addresses: &[Ipv4Addr]) -> Vec<Message> {
    let table = slot.table();
    let link = slot.link();
    let mut messages = vec![
        new_table(&table),
        new_set(&table),
        base_chain(
            &table,
            FORWARD,
            FILTER,
            InetHookNumber::Forward,
            FILTER_PRIORITY,
        ),
        base_chain(
            &table,
            INPUT,
            FILTER,
            InetHookNumber::LocalIn,
            FILTER_PRIORITY,
        ),
        base_chain(
            &table,
            POSTROUTING,
            NAT,
            InetHookNumber::PostRouting,
            SOURCE_NAT_PRIORITY,
        ),
    ];
    if !addresses.is_empty() {
        messages.push(elements(&table, addresses));
    }
    messages.extend(forward(&link).map(|rule| new_rule(&table, FORWARD, rule)));
    messages.push(new_rule(&table, INPUT, from(&link, Vec::new(), DROP)));
    messages.push(new_rule(&table, POSTROUTING, masquerade(slot, &link)));
    batch(messages)
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

/// Asks for every `inet` table.
pub(super) fn every() -> Message {
    nf(
        NfTablesMessage::GetTable(TableMessage {
            attributes: Vec::new(),
        }),
        0,
    )
}

/// Builds `slot`'s table in one transaction.
///
/// # Errors
/// The kernel refused the batch; nothing of it was applied.
pub(super) fn apply<W: Wire>(
    netfilter: &mut Netlink<W>,
    slot: Slot,
    addresses: &[Ipv4Addr],
) -> io::Result<()> {
    netfilter.acknowledged(install(slot, addresses))
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

fn new_table(table: &str) -> Message {
    let attributes = vec![TableAttribute::Name(table.to_owned())];
    nf(
        NfTablesMessage::NewTable(TableMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

fn new_set(table: &str) -> Message {
    let attributes = vec![
        SetAttribute::Table(table.to_owned()),
        SetAttribute::Name(SET.to_owned()),
        SetAttribute::KeyType(IPV4_ADDR_TYPE),
        SetAttribute::KeyLen(WORD),
        SetAttribute::Id(SET_ID),
    ];
    nf(
        NfTablesMessage::NewSet(SetMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

fn elements(table: &str, addresses: &[Ipv4Addr]) -> Message {
    let keys = addresses
        .iter()
        .map(|address| {
            ListAttribute::Element(vec![SetElementAttribute::Key(DataAttribute::Value(
                address.octets().to_vec(),
            ))])
        })
        .collect();
    let attributes = vec![
        SetElementList::Table(table.to_owned()),
        SetElementList::Set(SET.to_owned()),
        SetElementList::SetId(SET_ID),
        SetElementList::Elements(keys),
    ];
    nf(
        NfTablesMessage::NewSetElement(SetElementMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

fn base_chain(table: &str, name: &str, kind: &str, hook: InetHookNumber, priority: u32) -> Message {
    let attributes = vec![
        ChainAttribute::Table(table.to_owned()),
        ChainAttribute::Name(name.to_owned()),
        ChainAttribute::Hook(vec![Hook::Number(hook.into()), Hook::Priority(priority)]),
        ChainAttribute::Policy(ACCEPT),
        ChainAttribute::Type(kind.to_owned()),
    ];
    nf(
        NfTablesMessage::NewChain(ChainMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

fn new_rule(table: &str, chain: &str, expressions: Vec<Expression>) -> Message {
    let attributes = vec![
        RuleAttribute::Table(table.to_owned()),
        RuleAttribute::Chain(chain.to_owned()),
        RuleAttribute::Expressions(expressions),
    ];
    nf(
        NfTablesMessage::NewRule(RuleMessage { attributes }),
        NLM_F_CREATE | NLM_F_APPEND,
    )
}

/// `inner`, addressed to the `inet` family, asking an acknowledgement.
fn nf(inner: NfTablesMessage, flags: u16) -> Message {
    let header = NetfilterHeader::new(NetfilterProtoFamily::Inet, 0, 0);
    let mut message = NetlinkMessage::from(NetfilterMessage::new(header, inner));
    message.header.flags = NLM_F_REQUEST | NLM_F_ACK | flags;
    message
}

/// `messages` between a batch's begin and end, which ask no acknowledgement.
fn batch(messages: Vec<Message>) -> Vec<Message> {
    let control = |step| {
        let header = NetfilterHeader::new(NetfilterProtoFamily::Unspec, 0, NFTABLES);
        let mut message = NetlinkMessage::from(NetfilterMessage::new(header, step));
        message.header.flags = NLM_F_REQUEST;
        message
    };
    [control(ControlMessage::BatchBegin)]
        .into_iter()
        .chain(messages)
        .chain([control(ControlMessage::BatchEnd)])
        .collect()
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
