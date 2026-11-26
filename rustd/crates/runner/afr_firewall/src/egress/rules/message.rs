//! The `nf_tables` messages a batch is built of, the batch's two ends, and the
//! kernel numbers their fields hold.

use libc::c_int;
use netlink_packet_core::{
    NLM_F_ACK, NLM_F_APPEND, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST, NetlinkMessage,
};
use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, Hook, InetHookNumber, NfTablesMessage, RuleAttribute,
    RuleMessage, TableAttribute, TableFlags, TableMessage,
};
use netlink_packet_netfilter::none::ControlMessage;
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};

use super::Message;
use super::expressions::Expression;

/// The netfilter subsystem a batch is addressed to.
const NFTABLES: u16 = u16_of(libc::NFNL_SUBSYS_NFTABLES);

pub(super) fn new_table(table: &str, flags: TableFlags) -> Message {
    let attributes = vec![
        TableAttribute::Name(table.to_owned()),
        TableAttribute::Flags(flags),
    ];
    nf(
        NfTablesMessage::NewTable(TableMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

pub(super) fn base_chain(
    table: &str,
    name: &str,
    kind: &str,
    hook: InetHookNumber,
    priority: u32,
    policy: u32,
) -> Message {
    let attributes = vec![
        ChainAttribute::Table(table.to_owned()),
        ChainAttribute::Name(name.to_owned()),
        ChainAttribute::Hook(vec![Hook::Number(hook.into()), Hook::Priority(priority)]),
        ChainAttribute::Policy(policy),
        ChainAttribute::Type(kind.to_owned()),
    ];
    nf(
        NfTablesMessage::NewChain(ChainMessage { attributes }),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

pub(super) fn new_rule(table: &str, chain: &str, expressions: Vec<Expression>) -> Message {
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
pub(super) fn nf(inner: NfTablesMessage, flags: u16) -> Message {
    nf_in(NetfilterProtoFamily::Inet, inner, flags)
}

/// `inner`, addressed to `family`, asking an acknowledgement.
pub(super) fn nf_in(family: NetfilterProtoFamily, inner: NfTablesMessage, flags: u16) -> Message {
    let header = NetfilterHeader::new(family, 0, 0);
    let mut message = NetlinkMessage::from(NetfilterMessage::new(header, inner));
    message.header.flags = NLM_F_REQUEST | NLM_F_ACK | flags;
    message
}

/// `messages` between a batch's begin and end, which ask no acknowledgement.
pub(super) fn batch(messages: Vec<Message>) -> Vec<Message> {
    let control = |step| {
        let header = NetfilterHeader::new(NetfilterProtoFamily::Unspec, 0, NFTABLES);
        let mut message = NetlinkMessage::from(NetfilterMessage::new(header, step));
        message.header.flags = NLM_F_REQUEST;
        message
    };
    std::iter::once(control(ControlMessage::BatchBegin))
        .chain(messages)
        .chain([control(ControlMessage::BatchEnd)])
        .collect()
}

/// A kernel number `libc` spells as a C `int`, in the one byte a netlink
/// field holds it in.
///
/// # Panics
/// When `number` does not fit, which fails the build: every caller is a
/// `const`.
pub(in crate::egress) const fn u8_of(number: c_int) -> u8 {
    let [low, rest @ ..] = number.to_le_bytes();
    assert!(matches!(rest, [0, 0, 0]), "a kernel number past one byte");
    low
}

/// [`u8_of`], for a two-byte field.
///
/// # Panics
/// When `number` does not fit, which fails the build as [`u8_of`] does.
pub(in crate::egress) const fn u16_of(number: c_int) -> u16 {
    let [low, high, rest @ ..] = number.to_le_bytes();
    assert!(matches!(rest, [0, 0]), "a kernel number past two bytes");
    u16::from_le_bytes([low, high])
}

/// [`u8_of`], for a four-byte field: every `int` fits, so only a negative one,
/// a verdict such as `NFT_RETURN` among them, is refused rather than read as
/// its magnitude.
///
/// # Panics
/// When `number` is negative, which fails the build as [`u8_of`] does.
pub(in crate::egress) const fn u32_of(number: c_int) -> u32 {
    assert!(number >= 0, "a negative kernel number in an unsigned field");
    number.unsigned_abs()
}
