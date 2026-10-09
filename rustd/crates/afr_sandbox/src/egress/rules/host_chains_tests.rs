use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, Hook, HookNumber, InetHookNumber, NfTablesMessage,
    TableAttribute, TableMessage,
};
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};

use super::super::expressions::{ACCEPT, DROP};
use super::dropping;
use crate::egress::testing::chain;

/// A forward chain that drops by policy is named, in either family that
/// carries IPv4: ufw's and Docker's chains are `ip`, a hand-written one is
/// often `inet`.
#[test]
fn test_a_dropping_forward_chain_is_named_in_both_ipv4_families() {
    let inet = chain(
        NetfilterProtoFamily::Inet,
        "filter",
        "forward",
        InetHookNumber::Forward,
        DROP,
    );
    let ip = chain(
        NetfilterProtoFamily::IPv4,
        "filter",
        "FORWARD",
        InetHookNumber::Forward,
        DROP,
    );

    assert_eq!(dropping(inet).as_deref(), Some("inet filter forward"));
    assert_eq!(dropping(ip).as_deref(), Some("ip filter FORWARD"));
}

/// Only a forward chain that drops on IPv4 counts: one that accepts, one on
/// another hook, and one that sees IPv6 alone pass.
#[test]
fn test_chains_that_cannot_drop_the_sandboxs_forwarding_pass() {
    let passing = [
        chain(
            NetfilterProtoFamily::Inet,
            "filter",
            "forward",
            InetHookNumber::Forward,
            ACCEPT,
        ),
        chain(
            NetfilterProtoFamily::Inet,
            "filter",
            "input",
            InetHookNumber::LocalIn,
            DROP,
        ),
        chain(
            NetfilterProtoFamily::IPv6,
            "filter",
            "FORWARD",
            InetHookNumber::Forward,
            DROP,
        ),
    ];

    for message in passing {
        let shown = format!("{message:?}");
        assert_eq!(dropping(message), None, "{shown}");
    }
}

/// A dropping forward chain is named whatever order its attributes come in,
/// and a table listed in a family that carries IPv4 is no chain at all.
#[test]
fn test_a_chain_is_named_from_its_attributes_and_a_table_is_not_one() {
    let inet = || NetfilterHeader::new(NetfilterProtoFamily::Inet, 0, 0);
    let name_first = NfTablesMessage::NewChain(ChainMessage {
        attributes: vec![
            ChainAttribute::Name("forward".to_owned()),
            ChainAttribute::Table("filter".to_owned()),
            ChainAttribute::Hook(vec![Hook::Number(HookNumber::Inet(
                InetHookNumber::Forward,
            ))]),
            ChainAttribute::Policy(DROP),
        ],
    });
    let table = NfTablesMessage::NewTable(TableMessage {
        attributes: vec![TableAttribute::Name("filter".to_owned())],
    });

    let chain = dropping(NetfilterMessage::new(inet(), name_first));
    let not_a_chain = dropping(NetfilterMessage::new(inet(), table));

    assert_eq!(chain.as_deref(), Some("inet filter forward"));
    assert_eq!(not_a_chain, None);
}
