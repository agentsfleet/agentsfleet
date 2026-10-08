#![expect(
    clippy::unwrap_used,
    reason = "test module: a message the test built itself parses"
)]

use std::fs::File;
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::{AsFd as _, AsRawFd as _};

use netlink_packet_core::{NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::address::AddressAttribute;
use netlink_packet_route::link::{InfoData, InfoVeth, LinkAttribute, LinkFlags, LinkInfo};
use netlink_packet_route::route::{RouteAddress, RouteAttribute};

use super::{address, configure_peer, default_route, join, names, pair, remove, up};
use crate::egress::slot::Slot;
use crate::egress::testing::{
    DELLINK, Fake, GETLINK, NEWADDR, NEWLINK, NEWROUTE, Protocol, SETLINK, round_trip,
};

/// The pair is created whole, the host end named for the slot and the peer
/// created straight into the sandbox's namespace, and refused if either name
/// is already taken.
#[test]
fn test_the_pair_puts_its_peer_in_the_sandbox() {
    let netns = File::open("/dev/null").unwrap();
    let slot = Slot::new(4).unwrap();
    let message = pair(slot, netns.as_fd());
    let flags = message.header.flags;

    let RouteNetlinkMessage::NewLink(link) = round_trip(message) else {
        unreachable!("a pair is a new link")
    };
    let peer = link
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            LinkAttribute::LinkInfo(infos) => infos.iter().find_map(|info| match info {
                LinkInfo::Data(InfoData::Veth(InfoVeth::Peer(peer))) => Some(peer.clone()),
                _ => None,
            }),
            _ => None,
        })
        .unwrap();

    assert_eq!(flags, NLM_F_REQUEST | NLM_F_ACK | NLM_F_CREATE | NLM_F_EXCL);
    assert!(
        link.attributes
            .contains(&LinkAttribute::IfName("afv4".to_owned()))
    );
    assert!(
        peer.attributes
            .contains(&LinkAttribute::IfName("afp4".to_owned()))
    );
    assert!(
        peer.attributes
            .contains(&LinkAttribute::NetNsFd(netns.as_raw_fd()))
    );
}

/// Each end is a `/30` holding its one address, up, and the sandbox's only
/// route is through the host end.
#[test]
fn test_each_end_is_addressed_up_and_routed() {
    let host = Ipv4Addr::new(10, 69, 4, 1);

    let RouteNetlinkMessage::NewAddress(addressed) = round_trip(address(9, host)) else {
        unreachable!("an address")
    };
    let RouteNetlinkMessage::SetLink(raised) = round_trip(up(9)) else {
        unreachable!("a link change")
    };
    let RouteNetlinkMessage::NewRoute(routed) = round_trip(default_route(9, host)) else {
        unreachable!("a route")
    };

    assert_eq!(
        (addressed.header.index, addressed.header.prefix_len),
        (9, 30)
    );
    assert!(
        addressed
            .attributes
            .contains(&AddressAttribute::Local(IpAddr::V4(host)))
    );
    assert_eq!(
        (
            raised.header.index,
            raised.header.flags,
            raised.header.change_mask
        ),
        (9, LinkFlags::Up, LinkFlags::Up)
    );
    assert_eq!(routed.header.destination_prefix_length, 0, "everything");
    assert!(
        routed
            .attributes
            .contains(&RouteAttribute::Gateway(RouteAddress::Inet(host)))
    );
    assert!(routed.attributes.contains(&RouteAttribute::Oif(9)));
}

/// Joining creates the pair, then addresses and raises the host end it looked
/// up; the peer is configured where it lives, in the same order.
#[test]
fn test_both_ends_are_configured_in_order() {
    let kernel = Fake::default();
    let netns = File::open("/dev/null").unwrap();
    let slot = Slot::new(4).unwrap();

    join(&mut kernel.open_route(), slot, netns.as_fd()).unwrap();
    configure_peer(&mut kernel.open_route(), slot).unwrap();

    assert_eq!(
        kernel.seen_on(Protocol::Route),
        [
            NEWLINK, GETLINK, NEWADDR, SETLINK, GETLINK, NEWADDR, SETLINK, NEWROUTE
        ]
    );
}

/// A link already gone is removed; any other refusal is the kernel's answer.
#[test]
fn test_removing_a_link_tolerates_only_its_absence() {
    let gone = Fake::default().refusing(Protocol::Route, DELLINK, libc::ENODEV);
    let denied = Fake::default().refusing(Protocol::Route, DELLINK, libc::EPERM);

    remove(&mut gone.open_route(), "afv4").unwrap();
    let refused = remove(&mut denied.open_route(), "afv4").unwrap_err();

    assert_eq!(refused.raw_os_error(), Some(libc::EPERM));
}

/// The listing keeps the links carrying the prefix and nothing else.
#[test]
fn test_only_prefixed_links_are_listed() {
    let kernel = Fake::default().holding(&[], &["lo", "afv3", "eth0", "afv12"]);

    let listed = names(&mut kernel.open_route(), "afv").unwrap();

    assert_eq!(listed, ["afv3", "afv12"]);
}
