#![expect(
    clippy::unwrap_used,
    reason = "test module: a reply the test built itself parses"
)]

use std::collections::VecDeque;
use std::io;
use std::num::NonZeroI32;

use netlink_packet_core::{
    DoneMessage, ErrorMessage, NLM_F_ACK, NLM_F_REQUEST, NetlinkBuffer, NetlinkHeader,
    NetlinkMessage, NetlinkPayload,
};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::link::{LinkAttribute, LinkMessage};
use netlink_sys::protocols::NETLINK_ROUTE;

use super::{Netlink, Wire, split};
use crate::egress::testing::bytes;

/// A wire that answers with the datagrams a test scripted, and keeps what was
/// sent.
#[derive(Debug, Default)]
struct Scripted {
    sent: Vec<Vec<u8>>,
    replies: VecDeque<io::Result<Vec<u8>>>,
}

impl Wire for Scripted {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()> {
        self.sent.push(datagram.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> io::Result<Vec<u8>> {
        self.replies
            .pop_front()
            .unwrap_or_else(|| Err(io::ErrorKind::WouldBlock.into()))
    }
}

fn scripted(replies: Vec<Vec<u8>>) -> Netlink<Scripted> {
    Netlink::over(Scripted {
        sent: Vec::new(),
        replies: replies.into_iter().map(Ok).collect(),
    })
}

fn request(flags: u16) -> NetlinkMessage<RouteNetlinkMessage> {
    let mut message = NetlinkMessage::from(RouteNetlinkMessage::GetLink(LinkMessage::default()));
    message.header.flags = flags;
    message
}

fn answer(code: Option<i32>) -> Vec<u8> {
    let mut error = ErrorMessage::default();
    error.code = code.and_then(NonZeroI32::new);
    bytes(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Error(error),
    ))
}

fn named(name: &str) -> Vec<u8> {
    let mut link = LinkMessage::default();
    link.attributes = vec![LinkAttribute::IfName(name.to_owned())];
    bytes(NetlinkMessage::from(RouteNetlinkMessage::NewLink(link)))
}

fn done() -> Vec<u8> {
    bytes(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Done(DoneMessage::default()),
    ))
}

fn name_of(message: RouteNetlinkMessage) -> Option<String> {
    match message {
        RouteNetlinkMessage::NewLink(link) => {
            link.attributes
                .into_iter()
                .find_map(|attribute| match attribute {
                    LinkAttribute::IfName(name) => Some(name),
                    _ => None,
                })
        }
        _ => None,
    }
}

/// Every request asking an acknowledgement waits for one, however the replies
/// are split into datagrams; one that asks none is not waited for. All of them
/// leave in one datagram, numbered in turn.
#[test]
fn test_every_acknowledgement_is_waited_for() {
    let both = [answer(None), answer(None)].concat();
    let mut netlink = scripted(vec![answer(None), both]);
    let ask = NLM_F_REQUEST | NLM_F_ACK;

    netlink
        .acknowledged(vec![
            request(ask),
            request(NLM_F_REQUEST),
            request(ask),
            request(ask),
        ])
        .unwrap();

    let datagram = netlink.wire.sent.concat();
    let sequences: Vec<u32> = split::<RouteNetlinkMessage>(&datagram)
        .unwrap()
        .iter()
        .map(|message| message.header.sequence_number)
        .collect();
    assert_eq!(netlink.wire.sent.len(), 1, "one datagram");
    assert_eq!(sequences, [1, 2, 3, 4]);
    assert!(netlink.wire.replies.is_empty(), "every reply read");
}

/// A refusal ends the wait with the kernel's own reason.
#[test]
fn test_a_refusal_answers_its_errno() {
    let mut netlink = scripted(vec![answer(None), answer(Some(-libc::EEXIST))]);
    let ask = NLM_F_REQUEST | NLM_F_ACK;

    let refused = netlink
        .acknowledged(vec![request(ask), request(ask)])
        .unwrap_err();

    assert_eq!(refused.raw_os_error(), Some(libc::EEXIST));
}

/// A fetch answers the kernel's message, or its refusal, or "not found" when
/// the kernel ends without one.
#[test]
fn test_a_fetch_answers_one_message() {
    let found = scripted(vec![named("afv3")]).fetch(request(0)).unwrap();
    let refused = scripted(vec![answer(Some(-libc::ENODEV))])
        .fetch(request(0))
        .unwrap_err();
    let ended = scripted(vec![done()]).fetch(request(0)).unwrap_err();

    assert_eq!(name_of(found).as_deref(), Some("afv3"));
    assert_eq!(refused.raw_os_error(), Some(libc::ENODEV));
    assert_eq!(ended.kind(), io::ErrorKind::NotFound);
}

/// A dump gathers every message until the kernel says it is done, across as
/// many datagrams as it takes; a refusal mid-dump ends it.
#[test]
fn test_a_dump_gathers_until_done() {
    let first = [named("lo"), named("afv1")].concat();
    let mut netlink = scripted(vec![first, [named("afv2"), done()].concat()]);

    let names: Vec<_> = netlink
        .dump(request(0))
        .unwrap()
        .into_iter()
        .filter_map(name_of)
        .collect();
    let refused = scripted(vec![named("lo"), answer(Some(-libc::EPERM))]).dump(request(0));

    assert_eq!(names, ["lo", "afv1", "afv2"]);
    assert_eq!(refused.unwrap_err().raw_os_error(), Some(libc::EPERM));
}

/// A socket that fails, fails the conversation.
#[test]
fn test_a_failed_socket_fails_the_conversation() {
    let mut netlink = Netlink::over(Scripted {
        sent: Vec::new(),
        replies: VecDeque::from([Err(io::Error::from_raw_os_error(libc::ENOBUFS))]),
    });

    let failed = netlink.dump(request(0)).unwrap_err();

    assert_eq!(failed.raw_os_error(), Some(libc::ENOBUFS));
}

/// A message claiming more bytes than its datagram holds, or fewer than a
/// header, is refused rather than read past.
#[test]
fn test_a_malformed_datagram_is_refused() {
    let mut cut = named("afv1");
    cut.truncate(cut.len() - 4);
    let mut short = named("afv1");
    let length = u32::try_from(short.len() + 64).unwrap();
    NetlinkBuffer::new(&mut *short).set_length(length);

    split::<RouteNetlinkMessage>(&cut).unwrap_err();
    split::<RouteNetlinkMessage>(&short).unwrap_err();
    split::<RouteNetlinkMessage>(&[0; 3]).unwrap_err();
}

/// The real socket opens without privilege and lists this namespace's links,
/// loopback among them.
#[test]
fn test_a_route_socket_lists_the_links() {
    let mut route = Netlink::open(NETLINK_ROUTE).unwrap();

    let names: Vec<_> = route
        .dump(request(0))
        .unwrap()
        .into_iter()
        .filter_map(name_of)
        .collect();

    assert!(names.iter().any(|name| name == "lo"), "{names:?}");
}
