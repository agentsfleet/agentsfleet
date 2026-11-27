//! Loopback, raised in the guest's one network namespace over route netlink.
//!
//! A guest's kernel raises loopback only when it is booted with an address,
//! so in a guest with no network device a process dialling `127.0.0.1` reads
//! the network unreachable until `init` does this. One request names the link
//! by its name, so no lookup precedes it, and the kernel's acknowledgement is
//! its answer.

use std::io;

use netlink_packet_core::{NLM_F_ACK, NLM_F_REQUEST, NetlinkMessage, NetlinkPayload};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::link::{LinkAttribute, LinkFlags, LinkMessage};
use netlink_sys::protocols::NETLINK_ROUTE;
use netlink_sys::{Socket, SocketAddr};

/// The name every network namespace's loopback link is given.
const LOOPBACK: &str = "lo";

/// Brings loopback up in the calling thread's network namespace.
///
/// # Errors
/// The kernel refused the route socket or the change.
pub(super) fn raise() -> io::Result<()> {
    let mut socket = Socket::new(NETLINK_ROUTE)?;
    socket.bind_auto()?;
    socket.connect(&SocketAddr::new(0, 0))?;
    socket.send(&encoded(up(LOOPBACK)), 0)?;
    let (reply, _from) = socket.recv_from_full()?;
    acknowledged(&reply)
}

/// The request that brings the link named `name` up and asks for an
/// acknowledgement. An index of zero makes the kernel find it by its name.
fn up(name: &str) -> NetlinkMessage<RouteNetlinkMessage> {
    let mut link = LinkMessage::default();
    link.header.flags = LinkFlags::Up;
    link.header.change_mask = LinkFlags::Up;
    link.attributes = vec![LinkAttribute::IfName(name.to_owned())];
    let mut message = NetlinkMessage::from(RouteNetlinkMessage::SetLink(link));
    message.header.flags = NLM_F_REQUEST | NLM_F_ACK;
    message.header.sequence_number = 1;
    message
}

/// `message`, finished and serialized.
fn encoded(mut message: NetlinkMessage<RouteNetlinkMessage>) -> Vec<u8> {
    message.finalize();
    let mut bytes = vec![0; message.buffer_len()];
    message.serialize(&mut bytes);
    bytes
}

/// Whether `reply` is the kernel's acknowledgement rather than its refusal.
fn acknowledged(reply: &[u8]) -> io::Result<()> {
    let answer =
        NetlinkMessage::<RouteNetlinkMessage>::deserialize(reply).map_err(io::Error::other)?;
    match answer.payload {
        NetlinkPayload::Error(error) => error.code.map_or(Ok(()), |_code| Err(error.to_io())),
        _other => Err(io::Error::other(
            "the kernel answered loopback's change with no acknowledgement",
        )),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "a test asserts by panicking; the manifest's restriction set is for the agent"
    )]

    use netlink_packet_core::{ErrorMessage, NetlinkHeader, NetlinkMessage, NetlinkPayload};
    use netlink_packet_route::RouteNetlinkMessage;
    use netlink_packet_route::link::{LinkAttribute, LinkFlags};

    use super::{acknowledged, encoded, up};

    /// The one request names loopback and raises only its up flag.
    #[test]
    fn test_the_request_raises_the_link_named_lo_and_nothing_else() {
        let bytes = encoded(up("lo"));
        let parsed = NetlinkMessage::<RouteNetlinkMessage>::deserialize(&bytes).unwrap();

        let NetlinkPayload::InnerMessage(RouteNetlinkMessage::SetLink(link)) = parsed.payload
        else {
            panic!("not a link change: {:?}", parsed.payload);
        };
        assert_eq!(link.header.index, 0);
        assert_eq!(link.header.change_mask, LinkFlags::Up);
        assert_eq!(link.header.flags, LinkFlags::Up);
        assert_eq!(link.attributes, [LinkAttribute::IfName("lo".to_owned())]);
    }

    /// An acknowledgement passes, and the kernel's refusal is its errno.
    #[test]
    fn test_an_acknowledgement_passes_and_a_refusal_is_its_errno() {
        let answer = |code: i32| {
            let mut error = ErrorMessage::default();
            error.code = std::num::NonZeroI32::new(code);
            let mut message = NetlinkMessage::<RouteNetlinkMessage>::new(
                NetlinkHeader::default(),
                NetlinkPayload::Error(error),
            );
            message.finalize();
            let mut bytes = vec![0; message.buffer_len()];
            message.serialize(&mut bytes);
            bytes
        };

        acknowledged(&answer(0)).unwrap();
        let refused = acknowledged(&answer(-1)).unwrap_err();

        assert_eq!(refused.raw_os_error(), Some(1));
    }
}
