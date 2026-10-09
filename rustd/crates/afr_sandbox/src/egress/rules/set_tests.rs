#![expect(
    clippy::unwrap_used,
    reason = "test module: a batch the test built itself parses"
)]

use std::net::Ipv4Addr;

use netlink_packet_netfilter::NetfilterMessageInner;
use netlink_packet_netfilter::nftables::{
    DataAttribute, ListAttribute, NfTablesMessage, SetElementAttribute, SetElementList,
    SetElementMessage,
};

use super::{refill, refilled};
use crate::egress::rules::SET;
use crate::egress::slot::Slot;
use crate::egress::testing::{DELSETELEM, Fake, NEWSETELEM, Protocol, round_trip};

/// The addresses a held sandbox's names resolved to the second time.
const MOVED: [Ipv4Addr; 2] = [Ipv4Addr::new(10, 0, 0, 9), Ipv4Addr::new(10, 0, 0, 7)];

/// The `nf_tables` messages of `slot`'s refill to `addresses`, as the kernel
/// reads them, a batch's two ends left out.
fn read_back(slot: Slot, addresses: &[Ipv4Addr]) -> Vec<NfTablesMessage> {
    refilled(slot, addresses)
        .into_iter()
        .filter_map(|message| match round_trip(message).inner {
            NetfilterMessageInner::NfTables(message) => Some(message),
            _ => None,
        })
        .collect()
}

/// Every address an element list adds, in order.
fn keys(list: &SetElementMessage) -> Vec<Vec<u8>> {
    list.attributes
        .iter()
        .filter_map(|attribute| match attribute {
            SetElementList::Elements(elements) => Some(elements),
            _ => None,
        })
        .flatten()
        .filter_map(|element| match element {
            ListAttribute::Element(parts) => parts.iter().find_map(|part| match part {
                SetElementAttribute::Key(DataAttribute::Value(key)) => Some(key.clone()),
                _ => None,
            }),
            _ => None,
        })
        .collect()
}

/// A refill empties the set, then fills it with the new addresses, in one
/// batch: the deletion names no element, which empties the set whole, and
/// the set is found by name, since an earlier batch made it.
#[test]
fn test_a_refill_empties_the_set_then_fills_it_in_one_batch() {
    let messages = read_back(Slot::new(3).unwrap(), &MOVED);

    let [
        NfTablesMessage::DeleteSetElement(emptied),
        NfTablesMessage::NewSetElement(filled),
    ] = messages.as_slice()
    else {
        unreachable!("an emptying, then a filling: {messages:?}")
    };
    assert!(keys(emptied).is_empty(), "the deletion names no element");
    assert_eq!(keys(filled), MOVED.map(|address| address.octets().to_vec()));
    assert!(
        !filled
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, SetElementList::SetId(_))),
        "the set is found by name"
    );
}

/// Both messages of a refill name the slot's own table and the set its build
/// made, so a held sandbox's refill never touches another slot's addresses.
#[test]
fn test_a_refill_names_its_own_slots_table_and_set() {
    let slot = Slot::new(7).unwrap();

    for message in read_back(slot, &MOVED) {
        let (NfTablesMessage::DeleteSetElement(list) | NfTablesMessage::NewSetElement(list)) =
            &message
        else {
            unreachable!("a refill changes set elements alone: {message:?}")
        };
        let attributes = &list.attributes;
        assert!(
            attributes.contains(&SetElementList::Table(slot.table())),
            "{message:?}"
        );
        assert!(
            attributes.contains(&SetElementList::Set(SET.to_owned())),
            "{message:?}"
        );
    }
}

/// A refill to no address only empties the set: an empty list is no message.
#[test]
fn test_a_refill_to_nothing_only_empties_the_set() {
    let messages = read_back(Slot::new(3).unwrap(), &[]);

    assert!(
        matches!(messages.as_slice(), [NfTablesMessage::DeleteSetElement(_)]),
        "{messages:?}"
    );
}

/// The refill waits for the kernel's word on both messages, and a refusal of
/// either is the refill's.
#[test]
fn test_a_refill_is_acknowledged_or_refused_whole() {
    let slot = Slot::new(3).unwrap();
    let kernel = Fake::default();
    let refusing = Fake::default().refusing(Protocol::Netfilter, NEWSETELEM, libc::ENOMEM);

    refill(&mut kernel.open_netfilter(), slot, &MOVED).unwrap();
    let refused = refill(&mut refusing.open_netfilter(), slot, &MOVED).unwrap_err();

    assert_eq!(
        kernel.seen_on(Protocol::Netfilter),
        [DELSETELEM, NEWSETELEM]
    );
    assert_eq!(refused.raw_os_error(), Some(libc::ENOMEM));
}
