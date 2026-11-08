//! The set of addresses a slot may reach: made with its table, and refilled
//! in place when the names a held sandbox reaches resolve anew.

use std::io;
use std::net::Ipv4Addr;

use netlink_packet_core::{NLM_F_CREATE, NLM_F_EXCL};
use netlink_packet_netfilter::nftables::{
    DataAttribute, ListAttribute, NfTablesMessage, SetAttribute, SetElementAttribute,
    SetElementList, SetElementMessage, SetMessage,
};

use super::expressions::WORD;
use super::{Message, SET, SET_ID, batch, nf};
use crate::egress::netlink::{Netlink, Wire};
use crate::egress::slot::Slot;

/// The kernel's own number for the `ipv4_addr` data type.
const IPV4_ADDR_TYPE: u32 = 7;

/// The set, made in the batch that makes `table`.
pub(super) fn new_set(table: &str) -> Message {
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

/// `addresses`, added to the set made earlier in the same batch.
pub(super) fn elements(table: &str, addresses: &[Ipv4Addr]) -> Message {
    added(table, addresses, Some(SET_ID))
}

/// The batch that empties `slot`'s set and fills it with `addresses`. The
/// kernel applies it whole or not at all, so no packet meets half of each.
pub(super) fn refilled(slot: Slot, addresses: &[Ipv4Addr]) -> Vec<Message> {
    let table = slot.table();
    // A deletion naming no element empties the whole set.
    let attributes = vec![
        SetElementList::Table(table.clone()),
        SetElementList::Set(SET.to_owned()),
    ];
    let mut messages = vec![nf(
        NfTablesMessage::DeleteSetElement(SetElementMessage { attributes }),
        0,
    )];
    if !addresses.is_empty() {
        messages.push(added(&table, addresses, None));
    }
    batch(messages)
}

/// Replaces `slot`'s set with `addresses` in one transaction, through the
/// socket that owns its table.
///
/// # Errors
/// The kernel refused the batch; the set holds what it held before.
pub(in crate::egress) fn refill<W: Wire>(
    netfilter: &mut Netlink<W>,
    slot: Slot,
    addresses: &[Ipv4Addr],
) -> io::Result<()> {
    netfilter.acknowledged(refilled(slot, addresses))
}

/// `addresses` added to `table`'s set, found by the number its batch gave it
/// when `set_id` is one, by name otherwise.
fn added(table: &str, addresses: &[Ipv4Addr], set_id: Option<u32>) -> Message {
    let keys = addresses
        .iter()
        .map(|address| {
            ListAttribute::Element(vec![SetElementAttribute::Key(DataAttribute::Value(
                address.octets().to_vec(),
            ))])
        })
        .collect();
    let attributes = [
        SetElementList::Table(table.to_owned()),
        SetElementList::Set(SET.to_owned()),
    ]
    .into_iter()
    .chain(set_id.map(SetElementList::SetId))
    .chain([SetElementList::Elements(keys)])
    .collect();
    nf(
        NfTablesMessage::NewSetElement(SetElementMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

#[cfg(test)]
#[path = "set_tests.rs"]
mod tests;
