//! One netlink conversation: requests serialized back to back into one
//! datagram, and the kernel's replies read until the request is answered.
//!
//! What crosses the socket is behind [`Wire`], so every way a conversation
//! ends — every acknowledgement, a refusal mid-batch, a dump split across
//! datagrams — is proven without a kernel; only [`Netlink::open`] needs one.

use std::io;

use netlink_packet_core::{
    NLM_F_ACK, NLM_F_DUMP, NLM_F_REQUEST, NetlinkBuffer, NetlinkDeserializable, NetlinkMessage,
    NetlinkPayload, NetlinkSerializable,
};
use netlink_sys::{Socket, SocketAddr};

/// Netlink messages start on four-byte boundaries.
const ALIGN: usize = 4;

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
    /// an acknowledgement has one, or the first refusal.
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
        self.send(messages)?;
        while waiting > 0 {
            for reply in self.receive::<I>()? {
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
        self.send(vec![message])?;
        loop {
            for reply in self.receive::<I>()? {
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
        self.send(vec![message])?;
        let mut answers = Vec::new();
        loop {
            for reply in self.receive::<I>()? {
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
    fn send<I: NetlinkSerializable>(&mut self, messages: Vec<NetlinkMessage<I>>) -> io::Result<()> {
        let mut datagram = Vec::new();
        for mut message in messages {
            self.sequence = self.sequence.wrapping_add(1);
            message.header.sequence_number = self.sequence;
            message.finalize();
            let mut bytes = vec![0; message.buffer_len()];
            message.serialize(&mut bytes);
            datagram.extend(bytes);
            datagram.resize(datagram.len().next_multiple_of(ALIGN), 0);
        }
        self.wire.send(&datagram)
    }

    /// The messages in the next datagram.
    fn receive<I: NetlinkDeserializable>(&mut self) -> io::Result<Vec<NetlinkMessage<I>>> {
        split(&self.wire.recv()?)
    }
}

/// Every message in `datagram`, in order.
fn split<I: NetlinkDeserializable>(datagram: &[u8]) -> io::Result<Vec<NetlinkMessage<I>>> {
    let mut messages = Vec::new();
    let mut rest = datagram;
    while !rest.is_empty() {
        let length = NetlinkBuffer::new_checked(rest)
            .map_err(io::Error::other)?
            .length();
        let length = usize::try_from(length).map_err(io::Error::other)?;
        let (message, after) = rest.split_at_checked(length).ok_or_else(truncated)?;
        messages.push(NetlinkMessage::deserialize(message).map_err(io::Error::other)?);
        rest = after
            .get(length.next_multiple_of(ALIGN) - length..)
            .unwrap_or_default();
    }
    Ok(messages)
}

/// A message longer than the datagram that carried it.
fn truncated() -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "a netlink message ran past its datagram",
    )
}

/// The kernel's refusal, when `error` is one; an acknowledgement is not.
fn refused(error: &netlink_packet_core::ErrorMessage) -> io::Result<()> {
    error.code.map_or(Ok(()), |_code| Err(error.to_io()))
}

#[cfg(test)]
#[path = "netlink_tests.rs"]
mod tests;
