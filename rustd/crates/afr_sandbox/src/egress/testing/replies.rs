//! What a kernel says, as the bytes and messages a test or the fake kernel
//! answers with, and a message read back the way the kernel would read it.

#![expect(
    clippy::unwrap_used,
    reason = "test support: a message the test built itself parses"
)]

use std::fmt::Debug;
use std::num::NonZeroI32;

use netlink_packet_core::{
    DoneMessage, ErrorMessage, NetlinkDeserializable, NetlinkHeader, NetlinkMessage,
    NetlinkPayload, NetlinkSerializable,
};
use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, Hook, HookNumber, InetHookNumber, NfTablesMessage,
};
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::link::{LinkAttribute, LinkMessage};

use super::INDEX;
use crate::egress::netlink::encode;

/// An acknowledgement.
pub(in crate::egress) fn ack() -> Vec<u8> {
    answered(Vec::new(), None)
}

/// A refusal naming `errno`.
pub(in crate::egress) fn refusal(errno: i32) -> Vec<u8> {
    answered(Vec::new(), NonZeroI32::new(-errno))
}

/// The answer to the request whose header is `header`: its acknowledgement,
/// or, with a `code`, its refusal.
pub(super) fn answered(header: Vec<u8>, code: Option<NonZeroI32>) -> Vec<u8> {
    let mut answer = ErrorMessage::default();
    answer.code = code;
    answer.header = header;
    encode(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Error(answer),
    ))
}

/// The end of a dump.
pub(in crate::egress) fn done() -> Vec<u8> {
    encode(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Done(DoneMessage::default()),
    ))
}

/// A link named `name`, numbered [`INDEX`], as the kernel lists it.
pub(in crate::egress) fn new_link(name: &str) -> Vec<u8> {
    let mut link = LinkMessage::default();
    link.header.index = INDEX;
    link.attributes = vec![LinkAttribute::IfName(name.to_owned())];
    encode(NetlinkMessage::from(RouteNetlinkMessage::NewLink(link)))
}

/// A base chain on `hook`, as the kernel lists it.
pub(in crate::egress) fn chain(
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

/// `message` as the kernel would read it back.
pub(in crate::egress) fn round_trip<I>(message: NetlinkMessage<I>) -> I
where
    I: NetlinkSerializable + NetlinkDeserializable + Debug,
{
    match NetlinkMessage::<I>::deserialize(&encode(message))
        .unwrap()
        .payload
    {
        NetlinkPayload::InnerMessage(inner) => inner,
        other => unreachable!("not an inner message: {other:?}"),
    }
}
