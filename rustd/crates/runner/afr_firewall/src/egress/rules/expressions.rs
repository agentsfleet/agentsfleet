//! The expressions a slot's rules are made of: each `nft` phrase the module
//! above draws, as the registers and comparisons the kernel runs.

use netlink_packet_core::DefaultNla;
use netlink_packet_netfilter::nftables::{
    Bitwise, Cmp, DataAttribute, ExpressionAttribute, Expressions, Immediate, ListAttribute,
    Lookup, Meta, MetaKey, Operator, Payload, Register, Verdict, VerdictAttribute,
};

use std::net::Ipv4Addr;

use super::{Policy, RESERVED, Reach, SET, SET_ID, u8_of, u32_of};

/// One expression in a rule.
pub(super) type Expression = ListAttribute<ExpressionAttribute>;

/// The verdicts, as the kernel numbers them.
pub(in crate::egress) const DROP: u32 = u32_of(libc::NF_DROP);
pub(super) const ACCEPT: u32 = u32_of(libc::NF_ACCEPT);
/// Bytes in an IPv4 address, a set key, and a 32-bit register.
pub(super) const WORD: u32 = 4;
/// Where a payload load starts: the network header, or the transport one.
const NETWORK_HEADER: u32 = u32_of(libc::NFT_PAYLOAD_NETWORK_HEADER);
const TRANSPORT_HEADER: u32 = u32_of(libc::NFT_PAYLOAD_TRANSPORT_HEADER);
/// Offsets into an IPv4 header, and the destination port's in a TCP or UDP one.
const SOURCE_OFFSET: u32 = 12;
const DESTINATION_OFFSET: u32 = 16;
const PORT_OFFSET: u32 = 2;
const PORT_LEN: u32 = 2;
/// `meta nfproto ipv4`, and the two transports a resolver answers on.
const NFPROTO_IPV4: u8 = u8_of(libc::NFPROTO_IPV4);
const TCP: u8 = u8_of(libc::IPPROTO_TCP);
const UDP: u8 = u8_of(libc::IPPROTO_UDP);
/// The resolver port, closed to every sandbox whatever its set holds.
pub(in crate::egress) const DNS_PORT: u16 = 53;
/// The connection-tracking expression: its register and key attributes, and
/// the key that reads a connection's state.
const CT: &str = "ct";
const CT_DESTINATION_REGISTER: u16 = 1;
const CT_KEY: u16 = 2;
const CT_STATE: u32 = u32_of(libc::NFT_CT_STATE);
/// `established` and `related`, as the state register's bits hold them.
const ESTABLISHED_OR_RELATED: u32 = 0b110;
/// The expression that rewrites a source to the outgoing link's address.
const MASQUERADE: &str = "masq";

/// The forward chain's rules, in order: under `policy`, each reserved range
/// dropped first; then the resolver port closed and the set reached, or every
/// other IPv4 destination reached; everything else from the sandbox dropped,
/// and nothing reaching it that it did not start.
pub(super) fn forward(link: &str, policy: Policy) -> Vec<Vec<Expression>> {
    let port = |transport| {
        vec![
            meta(MetaKey::L4Proto),
            compare(Operator::Equal, vec![transport]),
            load(TRANSPORT_HEADER, PORT_OFFSET, PORT_LEN),
            compare(Operator::Equal, DNS_PORT.to_be_bytes().to_vec()),
        ]
    };
    let ipv4 = || {
        vec![
            meta(MetaKey::Nfproto),
            compare(Operator::Equal, vec![NFPROTO_IPV4]),
        ]
    };
    let answered = vec![
        connection_state(),
        mask(ESTABLISHED_OR_RELATED),
        compare(Operator::NotEqual, vec![0; 4]),
    ];
    let reserved = policy
        .reserved_dropped
        .then_some(RESERVED.iter())
        .into_iter()
        .flatten()
        .map(|&(network, prefix_len)| {
            let mut matches = ipv4();
            matches.extend(within(DESTINATION_OFFSET, network, prefix_len));
            from(link, matches, DROP)
        });
    let reach = match policy.reach {
        Reach::Listed => {
            let mut to_set = ipv4();
            to_set.extend([load(NETWORK_HEADER, DESTINATION_OFFSET, WORD), in_set()]);
            vec![
                from(link, port(TCP), DROP),
                from(link, port(UDP), DROP),
                from(link, to_set, ACCEPT),
            ]
        }
        Reach::Public => vec![from(link, ipv4(), ACCEPT)],
    };
    reserved
        .chain(reach)
        .chain([
            from(link, Vec::new(), DROP),
            to(link, answered, ACCEPT),
            to(link, Vec::new(), DROP),
        ])
        .collect()
}

/// `iifname link`, then `matches`, then `verdict`.
pub(super) fn from(link: &str, matches: Vec<Expression>, verdict: u32) -> Vec<Expression> {
    rule_on(MetaKey::Iifname, link, matches, verdict)
}

/// `oifname link`, then `matches`, then `verdict`.
fn to(link: &str, matches: Vec<Expression>, verdict: u32) -> Vec<Expression> {
    rule_on(MetaKey::Oifname, link, matches, verdict)
}

fn rule_on(key: MetaKey, link: &str, matches: Vec<Expression>, code: u32) -> Vec<Expression> {
    [meta(key), compare(Operator::Equal, interface(link))]
        .into_iter()
        .chain(matches)
        .chain([verdict(code)])
        .collect()
}

/// `ip saddr <network>/<prefix_len> oifname != link masquerade`: traffic
/// from that network leaves under the outgoing link's address, and only when
/// it leaves another link.
pub(super) fn masquerade(network: Ipv4Addr, prefix_len: u8, link: &str) -> Vec<Expression> {
    let mut rule = vec![
        meta(MetaKey::Nfproto),
        compare(Operator::Equal, vec![NFPROTO_IPV4]),
    ];
    rule.extend(within(SOURCE_OFFSET, network, prefix_len));
    rule.extend([
        meta(MetaKey::Oifname),
        compare(Operator::NotEqual, interface(link)),
        Expressions::Other {
            expression_type: MASQUERADE.to_owned(),
            attributes: Vec::new(),
        }
        .into(),
    ]);
    rule
}

/// The IPv4 address at `offset` in the network header, masked to
/// `prefix_len` bits and compared with `network`.
fn within(offset: u32, network: Ipv4Addr, prefix_len: u8) -> [Expression; 3] {
    let netmask = u32::MAX
        .checked_shl(32 - u32::from(prefix_len))
        .unwrap_or(0)
        .to_be_bytes();
    [
        load(NETWORK_HEADER, offset, WORD),
        Expressions::Bitwise(vec![
            Bitwise::SourceRegister(Register::Reg1),
            Bitwise::DestinationRegister(Register::Reg1),
            Bitwise::Length(WORD),
            Bitwise::Mask(DataAttribute::Value(netmask.to_vec())),
            Bitwise::Xor(DataAttribute::Value(vec![0; 4])),
        ])
        .into(),
        compare(Operator::Equal, network.octets().to_vec()),
    ]
}

fn meta(key: MetaKey) -> Expression {
    Expressions::Meta(vec![
        Meta::Key(key),
        Meta::DestinationRegister(Register::Reg1),
    ])
    .into()
}

fn compare(operator: Operator, data: Vec<u8>) -> Expression {
    Expressions::Cmp(vec![
        Cmp::SourceRegister(Register::Reg1),
        Cmp::Op(operator),
        Cmp::Data(DataAttribute::Value(data)),
    ])
    .into()
}

fn load(base: u32, offset: u32, len: u32) -> Expression {
    Expressions::Payload(vec![
        Payload::DestinationRegister(Register::Reg1),
        Payload::Base(base),
        Payload::Offset(offset),
        Payload::Len(len),
    ])
    .into()
}

fn in_set() -> Expression {
    Expressions::Lookup(vec![
        Lookup::SourceRegister(Register::Reg1),
        Lookup::Set(SET.to_owned()),
        Lookup::SetId(SET_ID),
    ])
    .into()
}

/// `ct state` into the first register.
fn connection_state() -> Expression {
    let register = u32::from(Register::Reg1);
    Expressions::Other {
        expression_type: CT.to_owned(),
        attributes: vec![
            DefaultNla::new(CT_DESTINATION_REGISTER, register.to_be_bytes().to_vec()),
            DefaultNla::new(CT_KEY, CT_STATE.to_be_bytes().to_vec()),
        ],
    }
    .into()
}

/// The first register, `bits` of it kept: the state register is host-ordered.
fn mask(bits: u32) -> Expression {
    #[cfg(target_endian = "little")]
    let host_order = bits.to_le_bytes();
    #[cfg(target_endian = "big")]
    let host_order = bits.to_be_bytes();
    Expressions::Bitwise(vec![
        Bitwise::SourceRegister(Register::Reg1),
        Bitwise::DestinationRegister(Register::Reg1),
        Bitwise::Length(WORD),
        Bitwise::Mask(DataAttribute::Value(host_order.to_vec())),
        Bitwise::Xor(DataAttribute::Value(vec![0; 4])),
    ])
    .into()
}

fn verdict(code: u32) -> Expression {
    Expressions::Immediate(vec![
        Immediate::DestinationRegister(Register::Verdict),
        Immediate::Data(DataAttribute::Verdict(vec![VerdictAttribute::Code(
            Verdict::Other(code),
        )])),
    ])
    .into()
}

/// An interface name as `meta iifname` holds it: padded with zeros, so the
/// comparison is of the whole name and `afv1` never matches `afv12`.
fn interface(name: &str) -> Vec<u8> {
    let mut padded = name.as_bytes().to_vec();
    // An interface name is compared over its whole buffer, padding included.
    padded.resize(libc::IFNAMSIZ, 0);
    padded
}
