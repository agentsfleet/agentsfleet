//! What keeps a sandbox contained: each rule of a slot's table ends in its
//! verdict, on its side of the link, in order.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a slot the test names itself is in range, and a batch it built has the rules it reads"
)]

use std::net::Ipv4Addr;

use netlink_packet_netfilter::nftables::{
    Cmp, DataAttribute, Expressions, Immediate, Meta, MetaKey, Operator, Verdict, VerdictAttribute,
};

use super::expressions::{ACCEPT, DROP};
use super::tests::{compared, read_back, rules, tables};
use super::{FORWARD, INPUT, POSTROUTING, install, install_guest_side};
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
/// link, in order: from the sandbox, a source not its own is dropped, the
/// resolver port is dropped on both
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

/// A sender on a link is the one address it was given: the tap's table drops
/// what the guest sends under any other source, and the host's table what the
/// slot's link carries under any but the slot's own, each as the first rule
/// of its forward chain, so no forged source is translated or accepted.
#[test]
fn test_a_forged_source_is_dropped_before_anything_from_the_link_is_accepted() {
    let guest = Ipv4Addr::new(172, 16, 0, 2);
    let network = (Ipv4Addr::new(172, 16, 0, 0), 30);
    let tap = read_back(install_guest_side("afguest", "tap0", guest, network));
    let host = read_back(install(Slot::new(7).unwrap(), &[]));

    for (batch, link, source) in [
        (tap, "tap0", [172, 16, 0, 2]),
        (host, "afv7", [10, 69, 7, 2]),
    ] {
        let rules = rules(&tables(&batch));
        let (chain, first) = &rules[0];
        let mut name = link.as_bytes().to_vec();
        name.resize(16, 0);
        let values: Vec<_> = first.iter().filter_map(compared).collect();
        let differs = first.iter().any(|expression| {
            matches!(expression, Expressions::Cmp(parts) if parts.contains(&Cmp::Op(Operator::NotEqual)))
        });

        assert_eq!(chain, "forward", "{link}");
        assert_eq!(values, [name, vec![2], source.to_vec()], "{link}");
        assert!(differs, "{link}: any source but its own");
        assert_eq!(ends_in(first), Some(DROP), "{link}");
    }
}
