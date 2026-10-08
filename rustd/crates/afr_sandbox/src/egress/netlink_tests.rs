#![expect(
    clippy::unwrap_used,
    reason = "test module: a reply the test built itself parses"
)]

use std::collections::VecDeque;
use std::io;

use netlink_packet_core::{
    ErrorMessage, NLM_F_ACK, NLM_F_REQUEST, NetlinkBuffer, NetlinkHeader, NetlinkMessage,
    NetlinkPayload,
};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::link::LinkMessage;
use netlink_sys::protocols::NETLINK_ROUTE;

use super::{Netlink, Wire, encode, frames, split};
use crate::egress::link::name_of;
use crate::egress::testing::{ack, answering, done, new_link as named, refusal};

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

/// Every request asking an acknowledgement waits for one, however the replies
/// are split into datagrams; one that asks none is not waited for. All of them
/// leave in one datagram, numbered in turn.
#[test]
fn test_every_acknowledgement_is_waited_for() {
    let both = [answering(3, ack()), answering(4, ack())].concat();
    let mut netlink = scripted(vec![answering(1, ack()), both]);
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
    let mut netlink = scripted(vec![
        answering(1, ack()),
        answering(2, refusal(libc::EEXIST)),
    ]);
    let ask = NLM_F_REQUEST | NLM_F_ACK;

    let refused = netlink
        .acknowledged(vec![request(ask), request(ask)])
        .unwrap_err();

    assert_eq!(refused.raw_os_error(), Some(libc::EEXIST));
}

/// A conversation reads only the replies numbered as its own requests: what an
/// earlier one left unread after its refusal, an acknowledgement or a refusal,
/// is passed over, whichever way the next conversation asks.
#[test]
fn test_a_reply_an_earlier_conversation_left_is_passed_over() {
    let ask = NLM_F_REQUEST | NLM_F_ACK;
    let left = [answering(2, ack()), answering(3, refusal(libc::EEXIST))].concat();
    let mut netlink = scripted(vec![
        answering(1, refusal(libc::EPERM)),
        left.clone(),
        answering(4, ack()),
        left,
        answering(5, named("afv3")),
        answering(3, done()),
        answering(6, [named("afv1"), done()].concat()),
    ]);

    let refused = netlink
        .acknowledged(vec![request(ask), request(ask), request(ask)])
        .unwrap_err();
    netlink.acknowledged(vec![request(ask)]).unwrap();
    let found = netlink.fetch(request(0)).unwrap();
    let names: Vec<_> = netlink
        .dump(request(0))
        .unwrap()
        .into_iter()
        .filter_map(name_of)
        .collect();

    assert_eq!(refused.raw_os_error(), Some(libc::EPERM));
    assert_eq!(name_of(found).as_deref(), Some("afv3"));
    assert_eq!(names, ["afv1"]);
    assert!(netlink.wire.replies.is_empty(), "every reply read");
}

/// A fetch answers the kernel's message, or its refusal, or "not found" when
/// the kernel ends without one.
#[test]
fn test_a_fetch_answers_one_message() {
    let found = scripted(vec![answering(1, named("afv3"))])
        .fetch(request(0))
        .unwrap();
    let refused = scripted(vec![answering(1, refusal(libc::ENODEV))])
        .fetch(request(0))
        .unwrap_err();
    let ended = scripted(vec![answering(1, done())])
        .fetch(request(0))
        .unwrap_err();

    assert_eq!(name_of(found).as_deref(), Some("afv3"));
    assert_eq!(refused.raw_os_error(), Some(libc::ENODEV));
    assert_eq!(ended.kind(), io::ErrorKind::NotFound);
}

/// A dump gathers every message until the kernel says it is done, across as
/// many datagrams as it takes; a refusal mid-dump ends it.
#[test]
fn test_a_dump_gathers_until_done() {
    let first = answering(1, [named("lo"), named("afv1")].concat());
    let mut netlink = scripted(vec![first, answering(1, [named("afv2"), done()].concat())]);

    let names: Vec<_> = netlink
        .dump(request(0))
        .unwrap()
        .into_iter()
        .filter_map(name_of)
        .collect();
    let refused = scripted(vec![answering(
        1,
        [named("lo"), refusal(libc::EPERM)].concat(),
    )])
    .dump(request(0));

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

/// A message whose length is not a multiple of four is padded to the next
/// one, and the frame after it starts past that padding.
#[test]
fn test_a_message_is_padded_and_the_next_frame_starts_past_it() {
    let mut echoed = ErrorMessage::default();
    echoed.header = vec![1, 2, 3];
    let odd = encode(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Error(echoed),
    ));
    let unpadded = usize::try_from(NetlinkBuffer::new(&odd).length()).unwrap();
    let datagram = [odd.clone(), ack()].concat();

    let lengths: Vec<usize> = frames(&datagram)
        .map(|frame| frame.unwrap().len())
        .collect();

    assert!(
        unpadded < odd.len() && odd.len().is_multiple_of(4),
        "{unpadded} in {}",
        odd.len()
    );
    assert_eq!(lengths, [unpadded, ack().len()]);
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
