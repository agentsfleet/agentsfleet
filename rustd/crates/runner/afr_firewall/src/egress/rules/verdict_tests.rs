//! What keeps a sandbox contained: each rule of a slot's table ends in its
//! verdict, on its side of the link, in order.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a slot the test names itself is in range"
)]

use netlink_packet_netfilter::nftables::{
    DataAttribute, Expressions, Immediate, Meta, MetaKey, Verdict, VerdictAttribute,
};

use super::expressions::{ACCEPT, DROP};
use super::tests::{read_back, rules, tables};
use super::{FORWARD, INPUT, POSTROUTING, install};
use crate::egress::slot::Slot;

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
