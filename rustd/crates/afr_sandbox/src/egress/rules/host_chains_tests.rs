use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, Hook, HookNumber, InetHookNumber, NfTablesMessage,
};
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};

use super::super::expressions::{ACCEPT, DROP};
use super::dropping;

/// A base chain as the kernel lists it.
fn chain(
    family: NetfilterProtoFamily,
    table: &str,
    name: &str,
    hook: InetHookNumber,
    policy: u32,
) -> NetfilterMessage {
    NetfilterMessage::new(
        NetfilterHeader::new(family, 0, 0),
        NfTablesMessage::NewChain(ChainMessage {
            attributes: vec![
                ChainAttribute::Table(table.to_owned()),
                ChainAttribute::Name(name.to_owned()),
                ChainAttribute::Hook(vec![
                    Hook::Number(HookNumber::Inet(hook)),
                    Hook::Priority(0),
                ]),
                ChainAttribute::Policy(policy),
            ],
        }),
    )
}

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
