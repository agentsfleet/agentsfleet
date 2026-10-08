#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a batch the test built itself parses, and has the rules it reads"
)]

use std::net::Ipv4Addr;

use netlink_packet_core::{NLM_F_ACK, NetlinkMessage, NetlinkPayload};
use netlink_packet_netfilter::nftables::{
    Bitwise, ChainAttribute, Cmp, DataAttribute, ExpressionAttribute, Expressions, Immediate,
    ListAttribute, Meta, MetaKey, NfTablesMessage, RuleAttribute, SetElementList, TableAttribute,
    TableFlags, Verdict, VerdictAttribute,
};
use netlink_packet_netfilter::{NetfilterMessage, NetfilterMessageInner};

use super::expressions::{ACCEPT, DROP};
use super::{FORWARD, INPUT, POSTROUTING, install, names, remove, uninstall};
use crate::egress::slot::Slot;
use crate::egress::testing::{DELTABLE, Fake, Protocol, bytes};

/// Every message of a batch, as the kernel would read it back.
fn read_back(batch: Vec<NetlinkMessage<NetfilterMessage>>) -> Vec<NetfilterMessageInner> {
    batch
        .into_iter()
        .map(|message| {
            let parsed = NetlinkMessage::<NetfilterMessage>::deserialize(&bytes(message)).unwrap();
            match parsed.payload {
                NetlinkPayload::InnerMessage(inner) => inner.inner,
                other => unreachable!("not a netfilter message: {other:?}"),
            }
        })
        .collect()
}

/// The `nf_tables` messages between a batch's two ends.
fn tables(inners: &[NetfilterMessageInner]) -> Vec<&NfTablesMessage> {
    inners
        .iter()
        .filter_map(|inner| match inner {
            NetfilterMessageInner::NfTables(message) => Some(message),
            _ => None,
        })
        .collect()
}

/// Each rule's chain and expressions, in order.
fn rules(messages: &[&NfTablesMessage]) -> Vec<(String, Vec<Expressions>)> {
    messages
        .iter()
        .filter_map(|message| match message {
            NfTablesMessage::NewRule(rule) => Some(rule),
            _ => None,
        })
        .map(|rule| {
            let mut chain = String::new();
            let mut expressions = Vec::new();
            for attribute in &rule.attributes {
                match attribute {
                    RuleAttribute::Chain(name) => chain.clone_from(name),
                    RuleAttribute::Expressions(list) => expressions = data(list),
                    _ => {}
                }
            }
            (chain, expressions)
        })
        .collect()
}

fn data(list: &[ListAttribute<ExpressionAttribute>]) -> Vec<Expressions> {
    list.iter()
        .filter_map(|element| match element {
            ListAttribute::Element(attributes) => {
                attributes.iter().find_map(|attribute| match attribute {
                    ExpressionAttribute::Data(expression) => Some(expression.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The value a comparison is made against.
fn compared(expression: &Expressions) -> Option<Vec<u8>> {
    match expression {
        Expressions::Cmp(parts) => parts.iter().find_map(|part| match part {
            Cmp::Data(DataAttribute::Value(value)) => Some(value.clone()),
            _ => None,
        }),
        _ => None,
    }
}

/// The batch builds one table whole: its set, filled; three chains accepting
/// by policy; six forward rules, one input rule and one translation rule, each
/// asking an acknowledgement and every one surviving the kernel's encoding.
#[test]
fn test_the_table_is_built_in_one_batch() {
    let slot = Slot::new(5).unwrap();
    let batch = install(
        slot,
        &[
            Ipv4Addr::new(104, 16, 0, 35),
            Ipv4Addr::new(151, 101, 0, 223),
        ],
    );
    let asking: Vec<bool> = batch
        .iter()
        .map(|message| message.header.flags & NLM_F_ACK != 0)
        .collect();

    let inners = read_back(batch);
    let messages = tables(&inners);
    let chains: Vec<_> = rules(&messages)
        .into_iter()
        .map(|(chain, _)| chain)
        .collect();

    assert!(
        matches!(inners.first(), Some(NetfilterMessageInner::None(_))),
        "begins"
    );
    assert!(
        matches!(inners.last(), Some(NetfilterMessageInner::None(_))),
        "ends"
    );
    assert_eq!(asking.iter().filter(|&&asks| asks).count(), messages.len());
    assert!(matches!(messages[0], NfTablesMessage::NewTable(_)));
    let policies = messages
        .iter()
        .filter_map(|message| match message {
            NfTablesMessage::NewChain(chain) => {
                chain
                    .attributes
                    .iter()
                    .find_map(|attribute| match attribute {
                        ChainAttribute::Policy(policy) => Some(*policy),
                        _ => None,
                    })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(policies, [1, 1, 1], "every chain accepts by policy");
    let keys = messages.iter().find_map(|message| match message {
        NfTablesMessage::NewSetElement(elements) => {
            elements
                .attributes
                .iter()
                .find_map(|attribute| match attribute {
                    SetElementList::Elements(keys) => Some(keys.len()),
                    _ => None,
                })
        }
        _ => None,
    });
    assert_eq!(keys, Some(2));
    assert_eq!(
        chains,
        [
            "forward",
            "forward",
            "forward",
            "forward",
            "forward",
            "forward",
            "input",
            "postrouting"
        ]
    );
}

/// An empty allowlist builds the table and set with nothing in the set: the
/// sandbox reaches nothing, and the kernel is never sent an empty element list.
/// The table asks to be owned by the socket that builds it, so no other
/// process may delete or flush it and it goes when that socket closes.
#[test]
fn test_the_table_is_owned_by_its_builder() {
    let inners = read_back(install(Slot::new(4).unwrap(), &[]));

    let owned = tables(&inners).iter().any(|message| {
        matches!(message, NfTablesMessage::NewTable(table)
            if table.attributes.contains(&TableAttribute::Flags(TableFlags::Owner)))
    });
    assert!(owned, "the new table carries NFT_TABLE_F_OWNER");
}

#[test]
fn test_an_empty_allowlist_fills_nothing() {
    let messages = read_back(install(Slot::new(5).unwrap(), &[]));

    assert!(
        !tables(&messages)
            .iter()
            .any(|message| matches!(message, NfTablesMessage::NewSetElement(_)))
    );
}

/// Every rule names the slot's link whole, padded as the kernel compares
/// names, so `afv1` never matches `afv12`; the resolver port is closed on both
/// transports before anything is accepted.
#[test]
fn test_every_rule_is_scoped_to_its_link() {
    let messages = read_back(install(Slot::new(1).unwrap(), &[]));
    let mut name = b"afv1".to_vec();
    name.resize(16, 0);

    let rules = rules(&tables(&messages));

    for (chain, expressions) in &rules {
        let names_link = expressions
            .iter()
            .filter_map(compared)
            .any(|value| value == name);
        assert!(names_link, "{chain}: {expressions:?}");
    }
    let dns: Vec<_> = rules[..2]
        .iter()
        .map(|(_, expressions)| (compared(&expressions[3]), compared(&expressions[5])))
        .collect();
    assert_eq!(
        dns,
        [
            (Some(vec![6]), Some(vec![0, 53])),
            (Some(vec![17]), Some(vec![0, 53]))
        ]
    );
}

/// The side of the link a rule reads: the first interface name it loads.
fn side(expressions: &[Expressions]) -> Option<MetaKey> {
    expressions.iter().find_map(|expression| match expression {
        Expressions::Meta(parts) => parts.iter().find_map(|part| match part {
            Meta::Key(key @ (MetaKey::Iifname | MetaKey::Oifname)) => Some(*key),
            _ => None,
        }),
        _ => None,
    })
}

/// The verdict a rule ends in, when it ends in one.
fn ends_in(expressions: &[Expressions]) -> Option<u32> {
    match expressions.last() {
        Some(Expressions::Immediate(parts)) => parts.iter().find_map(|part| match part {
            Immediate::Data(DataAttribute::Verdict(attributes)) => {
                attributes.iter().find_map(|attribute| match attribute {
                    VerdictAttribute::Code(Verdict::Other(code)) => Some(*code),
                    _ => None,
                })
            }
            _ => None,
        }),
        _ => None,
    }
}

/// What keeps a sandbox contained is each rule's verdict on its side of the
/// link, in order: from the sandbox, the resolver port is dropped on both
/// transports, the set accepted and the rest dropped; into it, only answered
/// traffic is accepted; and nothing from it reaches the host's own input. A
/// verdict flipped, or a rule reading the other side, fails here.
#[test]
fn test_each_rule_ends_in_its_verdict_on_its_side_of_the_link() {
    let messages = read_back(install(Slot::new(1).unwrap(), &[]));

    let shape: Vec<_> = rules(&tables(&messages))
        .into_iter()
        .map(|(chain, expressions)| (chain, side(&expressions), ends_in(&expressions)))
        .collect();

    let from = Some(MetaKey::Iifname);
    let to = Some(MetaKey::Oifname);
    let expected = [
        (FORWARD, from, Some(DROP)),
        (FORWARD, from, Some(DROP)),
        (FORWARD, from, Some(ACCEPT)),
        (FORWARD, from, Some(DROP)),
        (FORWARD, to, Some(ACCEPT)),
        (FORWARD, to, Some(DROP)),
        (INPUT, from, Some(DROP)),
        (POSTROUTING, to, None),
    ]
    .map(|(chain, side, verdict)| (chain.to_owned(), side, verdict));
    assert_eq!(shape, expected);
}

/// Translation takes only the slot's own `/30`, leaving another link: the
/// network compared is the slot's, under a `/30` mask.
#[test]
fn test_translation_takes_only_the_slots_network() {
    let messages = read_back(install(Slot::new(9).unwrap(), &[]));
    let rules = rules(&tables(&messages));
    let (_, translation) = rules.last().unwrap();

    let mask = translation.iter().find_map(|expression| match expression {
        Expressions::Bitwise(parts) => parts.iter().find_map(|part| match part {
            Bitwise::Mask(DataAttribute::Value(mask)) => Some(mask.clone()),
            _ => None,
        }),
        _ => None,
    });

    assert_eq!(mask, Some(vec![255, 255, 255, 252]));
    assert_eq!(compared(&translation[4]), Some(vec![10, 69, 9, 0]));
    assert!(
        matches!(translation.last(), Some(Expressions::Other { expression_type, .. }) if expression_type == "masq")
    );
}

/// Removal deletes the table whole; a table already gone is removed, any other
/// refusal is the kernel's answer.
#[test]
fn test_removing_a_table_tolerates_only_its_absence() {
    let removal = tables(&read_back(uninstall("afegress5"))).len();
    let gone = Fake::default().refusing(Protocol::Netfilter, DELTABLE, libc::ENOENT);
    let denied = Fake::default().refusing(Protocol::Netfilter, DELTABLE, libc::EPERM);

    remove(&mut gone.open_netfilter(), "afegress5").unwrap();
    let refused = remove(&mut denied.open_netfilter(), "afegress5").unwrap_err();

    assert_eq!(removal, 1);
    assert_eq!(refused.raw_os_error(), Some(libc::EPERM));
}

/// The listing keeps the tables carrying the prefix and nothing else.
#[test]
fn test_only_prefixed_tables_are_listed() {
    let kernel = Fake::default().holding(&["filter", "afegress2", "nat", "afegress40"], &[]);

    let listed = names(&mut kernel.open_netfilter(), "afegress").unwrap();

    assert_eq!(listed, ["afegress2", "afegress40"]);
}
