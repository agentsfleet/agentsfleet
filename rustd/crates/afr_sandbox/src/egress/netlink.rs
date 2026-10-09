//! One netlink conversation: requests serialized back to back into one
//! datagram, and the kernel's replies read until the request is answered.
//!
//! A conversation reads only the replies numbered as its own requests. One
//! that ends at a refusal leaves the kernel's later replies to that batch on
//! the socket, and a scope's socket outlives every conversation it carries, so
//! the next conversation would otherwise read one as its own answer.
//!
//! What crosses the socket is behind [`Wire`], so every way a conversation
//! ends — every acknowledgement, a refusal mid-batch, a dump split across
//! datagrams — is proven without a kernel; only [`Netlink::open`] needs one.

use std::io;

use netlink_packet_core::{
    NLM_F_ACK, NLM_F_DUMP, NLM_F_REQUEST, NLMSG_ALIGNTO, NetlinkBuffer, NetlinkDeserializable,
    NetlinkMessage, NetlinkPayload, NetlinkSerializable,
};
use netlink_sys::{Socket, SocketAddr};

/// Where a conversation's bytes go and come from.
pub(crate) trait Wire {
    /// Sends one datagram.
    ///
    /// # Errors
    /// The socket refused it.
    fn send(&mut self, datagram: &[u8]) -> io::Result<()>;

    /// Receives one datagram.
    ///
    /// # Errors
    /// The socket failed.
    fn recv(&mut self) -> io::Result<Vec<u8>>;
}

impl Wire for Socket {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()> {
        Socket::send(self, datagram, 0).map(drop)
    }

    fn recv(&mut self) -> io::Result<Vec<u8>> {
        self.recv_from_full().map(|(datagram, _from)| datagram)
    }
}

/// A conversation with one netlink protocol.
#[derive(Debug)]
pub(crate) struct Netlink<W> {
    wire: W,
    sequence: u32,
}

impl Netlink<Socket> {
    /// A socket speaking `protocol` to the kernel, in the network namespace of
    /// the calling thread.
    ///
    /// # Errors
    /// The kernel refused the socket.
    pub(crate) fn open(protocol: isize) -> io::Result<Self> {
        let mut socket = Socket::new(protocol)?;
        socket.bind_auto()?;
        socket.connect(&SocketAddr::new(0, 0))?;
        Ok(Self::over(socket))
    }
}

impl<W: Wire> Netlink<W> {
    /// A conversation over `wire`.
    pub(crate) const fn over(wire: W) -> Self {
        Self { wire, sequence: 0 }
    }

    /// Sends `messages` as one datagram and waits until each one that asks for
    /// an acknowledgement has one, or the first refusal. Replies the kernel
    /// sends after that refusal stay unread, and a later conversation passes
    /// over them by their numbers rather than waiting here for them: a batch
    /// the kernel refuses whole is answered once, on its first message, so
    /// waiting for every message's reply could wait forever.
    ///
    /// # Errors
    /// The kernel refused a message, or the socket failed.
    pub(crate) fn acknowledged<I>(&mut self, messages: Vec<NetlinkMessage<I>>) -> io::Result<()>
    where
        I: NetlinkSerializable + NetlinkDeserializable,
    {
        let mut waiting = messages
            .iter()
            .filter(|message| message.header.flags & NLM_F_ACK != 0)
            .count();
        let sent = self.send(messages)?;
        while waiting > 0 {
            for reply in self.receive::<I>(sent)? {
                if let NetlinkPayload::Error(error) = reply.payload {
                    refused(&error)?;
                    waiting = waiting.saturating_sub(1);
                }
            }
        }
        Ok(())
    }

    /// Sends `message` as a request and returns the one message the kernel
    /// answers it with.
    ///
    /// # Errors
    /// The kernel refused it or answered nothing, or the socket failed.
    pub(crate) fn fetch<I>(&mut self, mut message: NetlinkMessage<I>) -> io::Result<I>
    where
        I: NetlinkSerializable + NetlinkDeserializable,
    {
        message.header.flags = NLM_F_REQUEST;
        let sent = self.send(vec![message])?;
        loop {
            for reply in self.receive::<I>(sent)? {
                match reply.payload {
                    NetlinkPayload::InnerMessage(answer) => return Ok(answer),
                    NetlinkPayload::Error(error) => refused(&error)?,
                    NetlinkPayload::Done(_) => return Err(io::ErrorKind::NotFound.into()),
                    _noop_or_overrun => {}
                }
            }
        }
    }

    /// Sends `message` as a dump request and returns every message the kernel
    /// answers it with, however many datagrams they span.
    ///
    /// # Errors
    /// The kernel refused it, or the socket failed.
    pub(crate) fn dump<I>(&mut self, mut message: NetlinkMessage<I>) -> io::Result<Vec<I>>
    where
        I: NetlinkSerializable + NetlinkDeserializable,
    {
        message.header.flags = NLM_F_REQUEST | NLM_F_DUMP;
        let sent = self.send(vec![message])?;
        let mut answers = Vec::new();
        loop {
            for reply in self.receive::<I>(sent)? {
                match reply.payload {
                    NetlinkPayload::InnerMessage(answer) => answers.push(answer),
                    NetlinkPayload::Done(_) => return Ok(answers),
                    NetlinkPayload::Error(error) => refused(&error)?,
                    _noop_or_overrun => {}
                }
            }
        }
    }

    /// Numbers each message, then sends them all as one datagram: an
    /// `nf_tables` batch is one transaction only when it arrives whole.
    fn send<I: NetlinkSerializable>(
        &mut self,
        messages: Vec<NetlinkMessage<I>>,
    ) -> io::Result<Sent> {
        let mut sent = Sent {
            first: self.sequence.wrapping_add(1),
            count: 0,
        };
        let mut datagram = Vec::new();
        for mut message in messages {
            self.sequence = self.sequence.wrapping_add(1);
            message.header.sequence_number = self.sequence;
            sent.count = sent.count.wrapping_add(1);
            datagram.extend(encode(message));
        }
        self.wire.send(&datagram).map(|()| sent)
    }

    /// The messages in the next datagram that answer `sent`; one an earlier
    /// conversation left unread is passed over.
    fn receive<I: NetlinkDeserializable>(
        &mut self,
        sent: Sent,
    ) -> io::Result<Vec<NetlinkMessage<I>>> {
        let mut replies = split(&self.wire.recv()?)?;
        replies.retain(|reply| sent.numbered(reply.header.sequence_number));
        Ok(replies)
    }
}

/// The numbers one conversation's requests went out under: `count` of them
/// from `first`, in turn, wrapping past `u32::MAX` as the counter does.
#[derive(Debug, Clone, Copy)]
struct Sent {
    first: u32,
    count: u32,
}

impl Sent {
    /// Whether a reply numbered `sequence` answers one of these requests: the
    /// kernel numbers each reply as the request it answers.
    const fn numbered(self, sequence: u32) -> bool {
        sequence.wrapping_sub(self.first) < self.count
    }
}

/// `message`, finished and serialized, padded to where a message after it
/// in the same datagram would start.
pub(super) fn encode<I: NetlinkSerializable>(mut message: NetlinkMessage<I>) -> Vec<u8> {
    message.finalize();
    let length = message.buffer_len();
    let mut bytes = vec![0; length];
    message.serialize(&mut bytes);
    bytes.resize(length.next_multiple_of(usize::from(NLMSG_ALIGNTO)), 0);
    bytes
}

/// Every message in `datagram`, in order.
fn split<I: NetlinkDeserializable>(datagram: &[u8]) -> io::Result<Vec<NetlinkMessage<I>>> {
    frames(datagram)
        .map(|frame| NetlinkMessage::deserialize(frame?).map_err(io::Error::other))
        .collect()
}

/// The bytes of each message in `datagram`, in order, ending at the first
/// that does not fit.
pub(super) fn frames(datagram: &[u8]) -> impl Iterator<Item = io::Result<&[u8]>> {
    let mut rest = datagram;
    std::iter::from_fn(move || {
        (!rest.is_empty()).then(|| {
            let framed = frame(rest);
            rest = framed.as_ref().map_or(&[][..], |&(_message, after)| after);
            framed.map(|(message, _after)| message)
        })
    })
}

/// The first message in `rest`, and what follows its padding.
fn frame(rest: &[u8]) -> io::Result<(&[u8], &[u8])> {
    // `new_checked` refuses a length shorter than a header or longer than
    // `rest`, so the split below always lands inside it.
    let length = NetlinkBuffer::new_checked(rest)
        .map_err(io::Error::other)?
        .length();
    let length = usize::try_from(length).map_err(io::Error::other)?;
    let (message, after) = rest
        .split_at_checked(length)
        .ok_or(io::ErrorKind::UnexpectedEof)?;
    let padding = length.next_multiple_of(usize::from(NLMSG_ALIGNTO)) - length;
    Ok((message, after.get(padding..).unwrap_or_default()))
}

/// The kernel's refusal, when `error` is one; an acknowledgement is not.
fn refused(error: &netlink_packet_core::ErrorMessage) -> io::Result<()> {
    error.code.map_or(Ok(()), |_code| Err(error.to_io()))
}

#[cfg(test)]
#[path = "netlink_tests.rs"]
mod tests;
