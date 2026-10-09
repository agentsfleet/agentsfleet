//! The veth pair that joins a sandbox's namespace to the host, over route
//! netlink: the messages, pure, and the few conversations made of them.

use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::{AsRawFd as _, BorrowedFd};

use netlink_packet_core::{NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST, NetlinkMessage};
use netlink_packet_route::address::{AddressAttribute, AddressMessage};
use netlink_packet_route::link::{
    InfoData, InfoKind, InfoVeth, LinkAttribute, LinkFlags, LinkInfo, LinkMessage,
};
use netlink_packet_route::route::{
    RouteAddress, RouteAttribute, RouteHeader, RouteMessage, RouteProtocol, RouteScope, RouteType,
};
use netlink_packet_route::{AddressFamily, RouteNetlinkMessage};

use super::netlink::{Netlink, Wire};
use super::slot::{PREFIX_LEN, Slot};

/// One route-netlink message.
pub(super) type Message = NetlinkMessage<RouteNetlinkMessage>;

/// Creates `slot`'s pair: its host end in the caller's namespace, its peer
/// created straight into `netns`, so the peer never exists on the host.
pub(super) fn pair(slot: Slot, netns: BorrowedFd<'_>) -> Message {
    named_pair(&slot.link(), &slot.peer(), netns)
}

/// Creates a veth pair, `name` in the caller's namespace and `peer` in `netns`.
pub(super) fn named_pair(name: &str, peer_name: &str, netns: BorrowedFd<'_>) -> Message {
    let mut peer = LinkMessage::default();
    peer.attributes = vec![
        LinkAttribute::IfName(peer_name.to_owned()),
        LinkAttribute::NetNsFd(netns.as_raw_fd()),
    ];
    let mut link = LinkMessage::default();
    link.attributes = vec![
        LinkAttribute::IfName(name.to_owned()),
        LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::Veth),
            LinkInfo::Data(InfoData::Veth(InfoVeth::Peer(peer))),
        ]),
    ];
    request(
        RouteNetlinkMessage::NewLink(link),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

/// Asks for the link named `name`.
pub(super) fn named(name: &str) -> Message {
    request(RouteNetlinkMessage::GetLink(by_name(name)), 0)
}

/// Gives the link numbered `index` `address`, in its slot's `/30`.
pub(super) fn address(index: u32, address: Ipv4Addr) -> Message {
    address_within(index, address, PREFIX_LEN)
}

/// Gives the link numbered `index` `address`, on a network of `prefix_len`.
pub(super) fn address_within(index: u32, address: Ipv4Addr, prefix_len: u8) -> Message {
    let mut message = AddressMessage::default();
    message.header.family = AddressFamily::Inet;
    message.header.prefix_len = prefix_len;
    message.header.index = index;
    message.attributes = vec![
        AddressAttribute::Local(IpAddr::V4(address)),
        AddressAttribute::Address(IpAddr::V4(address)),
    ];
    request(
        RouteNetlinkMessage::NewAddress(message),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

/// Brings the link numbered `index` up.
pub(super) fn up(index: u32) -> Message {
    let mut link = LinkMessage::default();
    link.header.index = index;
    link.header.flags = LinkFlags::Up;
    link.header.change_mask = LinkFlags::Up;
    request(RouteNetlinkMessage::SetLink(link), 0)
}

/// Sends everything off the link numbered `index` through `gateway`.
pub(super) fn default_route(index: u32, gateway: Ipv4Addr) -> Message {
    let mut route = RouteMessage::default();
    route.header.address_family = AddressFamily::Inet;
    route.header.table = RouteHeader::RT_TABLE_MAIN;
    route.header.protocol = RouteProtocol::Static;
    route.header.scope = RouteScope::Universe;
    route.header.kind = RouteType::Unicast;
    route.attributes = vec![
        RouteAttribute::Gateway(RouteAddress::Inet(gateway)),
        RouteAttribute::Oif(index),
    ];
    request(
        RouteNetlinkMessage::NewRoute(route),
        NLM_F_CREATE | NLM_F_EXCL,
    )
}

/// Deletes the link named `name`; a veth's peer goes with it.
pub(super) fn delete(name: &str) -> Message {
    request(RouteNetlinkMessage::DelLink(by_name(name)), 0)
}

/// Asks for every link in the namespace.
pub(super) fn every() -> Message {
    request(RouteNetlinkMessage::GetLink(LinkMessage::default()), 0)
}

/// Creates `slot`'s pair with its peer in `netns`, then addresses the host
/// end and brings it up.
///
/// # Errors
/// The kernel refused a step; what was made is the caller's to remove.
pub(super) fn join<W: Wire>(
    route: &mut Netlink<W>,
    slot: Slot,
    netns: BorrowedFd<'_>,
) -> io::Result<()> {
    route.acknowledged(vec![pair(slot, netns)])?;
    let index = index_named(route, &slot.link())?;
    route.acknowledged(vec![address(index, slot.host()), up(index)])
}

/// Addresses the peer, brings it up and routes everything through the host
/// end: run in the sandbox's namespace, the only place the peer is.
///
/// # Errors
/// The kernel refused a step.
pub(super) fn configure_peer<W: Wire>(route: &mut Netlink<W>, slot: Slot) -> io::Result<()> {
    let index = index_named(route, &slot.peer())?;
    route.acknowledged(vec![
        address(index, slot.sandbox()),
        up(index),
        default_route(index, slot.host()),
    ])
}

/// Deletes the link named `name`; one already gone is not a failure.
///
/// # Errors
/// The kernel refused for any other reason.
pub(super) fn remove<W: Wire>(route: &mut Netlink<W>, name: &str) -> io::Result<()> {
    match route.acknowledged(vec![delete(name)]) {
        Err(error) if error.raw_os_error() == Some(libc::ENODEV) => Ok(()),
        removed => removed,
    }
}

/// The name of every link in the namespace starting with `prefix`.
///
/// # Errors
/// The kernel refused the dump.
pub(super) fn names<W: Wire>(route: &mut Netlink<W>, prefix: &str) -> io::Result<Vec<String>> {
    let links = route.dump(every())?;
    Ok(links
        .into_iter()
        .filter_map(name_of)
        .filter(|name| name.starts_with(prefix))
        .collect())
}

/// The index of the link named `name`.
pub(super) fn index_named<W: Wire>(route: &mut Netlink<W>, name: &str) -> io::Result<u32> {
    match route.fetch(named(name))? {
        RouteNetlinkMessage::NewLink(link) => Ok(link.header.index),
        _ => Err(io::ErrorKind::InvalidData.into()),
    }
}

/// A link message naming `name` and nothing else.
fn by_name(name: &str) -> LinkMessage {
    let mut link = LinkMessage::default();
    link.attributes = vec![LinkAttribute::IfName(name.to_owned())];
    link
}

/// The name a link carries; none for any other message.
pub(super) fn name_of(message: RouteNetlinkMessage) -> Option<String> {
    let RouteNetlinkMessage::NewLink(link) = message else {
        return None;
    };
    link.attributes
        .into_iter()
        .find_map(|attribute| match attribute {
            LinkAttribute::IfName(name) => Some(name),
            _ => None,
        })
}

/// `inner` as a request asking an acknowledgement, with `flags` besides.
fn request(inner: RouteNetlinkMessage, flags: u16) -> Message {
    let mut message = NetlinkMessage::from(inner);
    message.header.flags = NLM_F_REQUEST | NLM_F_ACK | flags;
    message
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
