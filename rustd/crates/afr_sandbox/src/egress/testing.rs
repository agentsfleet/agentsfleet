//! A kernel a test states: it acknowledges every request, answers the link
//! lookups and the dumps a scope makes, and refuses whatever the test names,
//! so a scope's build, removal and sweep are proven without root.

#![expect(
    clippy::unwrap_used,
    reason = "test support: a request the fake cannot read is a broken test"
)]

use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::num::NonZeroI32;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use netlink_packet_core::{
    DoneMessage, ErrorMessage, NLM_F_ACK, NLM_F_DUMP, NetlinkBuffer, NetlinkHeader, NetlinkMessage,
    NetlinkPayload, NetlinkSerializable,
};
use netlink_packet_netfilter::nftables::{
    ChainAttribute, ChainMessage, Hook, HookNumber, InetHookNumber, NfTablesMessage,
    TableAttribute, TableMessage,
};
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};
use netlink_packet_route::RouteNetlinkMessage;
use netlink_packet_route::link::{LinkAttribute, LinkMessage};

use super::kernel::Kernel;
use super::netlink::{Netlink, Wire};

/// The index every link lookup answers.
pub(super) const INDEX: u32 = 7;
/// The route messages a scope sends, by type.
pub(super) const NEWLINK: u16 = 16;
pub(super) const DELLINK: u16 = 17;
pub(super) const GETLINK: u16 = 18;
pub(super) const SETLINK: u16 = 19;
pub(super) const NEWADDR: u16 = 20;
pub(super) const NEWROUTE: u16 = 24;
/// The `nf_tables` messages a scope sends, by type: subsystem 10, then the
/// message.
pub(super) const NEWTABLE: u16 = 0x0a00;
pub(super) const GETTABLE: u16 = 0x0a01;
pub(super) const DELTABLE: u16 = 0x0a02;
pub(super) const GETCHAIN: u16 = 0x0a04;
/// The policy a dropping chain is listed with (`NF_DROP`).
const DROP_POLICY: u32 = 0;
/// An `nf_tables` batch's two ends, which are answered by nothing.
const BATCH: [u16; 2] = [0x10, 0x11];
/// Bytes in a netlink header.
const HEADER_LEN: usize = 16;

/// Which protocol a socket speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Protocol {
    Route,
    Netfilter,
}

#[derive(Debug, Default)]
struct State {
    seen: Vec<(Protocol, u16)>,
    refusals: Vec<(Protocol, u16, i32)>,
    closed: Vec<Protocol>,
    tables: Vec<String>,
    links: Vec<String>,
    /// Forward base chains that drop by policy: family, table, chain.
    dropping: Vec<(NetfilterProtoFamily, String, String)>,
    entered: usize,
}

/// The kernel a test states; clones share one state.
#[derive(Debug, Clone, Default)]
pub(super) struct Fake(Arc<Mutex<State>>);

impl Fake {
    /// Every `message_type` on `protocol` answers `errno`.
    pub(super) fn refusing(self, protocol: Protocol, message_type: u16, errno: i32) -> Self {
        self.state().refusals.push((protocol, message_type, errno));
        self
    }

    /// No socket on `protocol` opens.
    pub(super) fn closed(self, protocol: Protocol) -> Self {
        self.state().closed.push(protocol);
        self
    }

    /// The dumps answer these tables and links.
    pub(super) fn holding(self, tables: &[&str], links: &[&str]) -> Self {
        let mut state = self.state();
        state.tables = tables.iter().map(|&name| name.to_owned()).collect();
        state.links = links.iter().map(|&name| name.to_owned()).collect();
        drop(state);
        self
    }

    /// The chain dump also lists a forward base chain in `table`, of `family`,
    /// whose policy drops.
    pub(super) fn dropping_forward(
        self,
        family: NetfilterProtoFamily,
        table: &str,
        chain: &str,
    ) -> Self {
        self.state()
            .dropping
            .push((family, table.to_owned(), chain.to_owned()));
        self
    }

    /// Every request received, in order, a batch's two ends left out.
    pub(super) fn seen(&self) -> Vec<(Protocol, u16)> {
        self.state().seen.clone()
    }

    /// The types of every request received on `protocol`, in order.
    pub(super) fn seen_on(&self, protocol: Protocol) -> Vec<u16> {
        self.seen()
            .into_iter()
            .filter_map(|(on, message_type)| (on == protocol).then_some(message_type))
            .collect()
    }

    /// How many times a step ran inside another namespace.
    pub(super) fn entered(&self) -> usize {
        self.state().entered
    }

    /// A route conversation with it.
    pub(super) fn open_route(&self) -> Netlink<FakeWire> {
        self.open(Protocol::Route).unwrap()
    }

    /// An `nf_tables` conversation with it.
    pub(super) fn open_netfilter(&self) -> Netlink<FakeWire> {
        self.open(Protocol::Netfilter).unwrap()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn open(&self, protocol: Protocol) -> io::Result<Netlink<FakeWire>> {
        if self.state().closed.contains(&protocol) {
            return Err(io::Error::from_raw_os_error(libc::EPROTONOSUPPORT));
        }
        Ok(Netlink::over(FakeWire {
            protocol,
            kernel: self.clone(),
            replies: VecDeque::new(),
        }))
    }
}

impl Kernel for Fake {
    type Wire = FakeWire;

    fn route(&self) -> io::Result<Netlink<FakeWire>> {
        self.open(Protocol::Route)
    }

    fn netfilter(&self) -> io::Result<Netlink<FakeWire>> {
        self.open(Protocol::Netfilter)
    }

    fn inside<T, E>(
        &self,
        _netns: BorrowedFd<'_>,
        step: impl FnOnce() -> Result<T, E> + Send,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<io::Error> + Send,
    {
        self.state().entered += 1;
        step()
    }

    fn fresh_namespace(&self) -> io::Result<OwnedFd> {
        Ok(File::open("/dev/null")?.into())
    }
}

/// One socket to the fake kernel.
#[derive(Debug)]
pub(super) struct FakeWire {
    protocol: Protocol,
    kernel: Fake,
    replies: VecDeque<Vec<u8>>,
}

impl Wire for FakeWire {
    fn send(&mut self, datagram: &[u8]) -> io::Result<()> {
        let mut rest = datagram;
        while let Ok(buffer) = NetlinkBuffer::new_checked(rest) {
            let (message_type, flags) = (buffer.message_type(), buffer.flags());
            let length = usize::try_from(buffer.length()).unwrap();
            let header = rest.get(..HEADER_LEN).unwrap().to_vec();
            rest = rest.get(length.next_multiple_of(4)..).unwrap_or_default();
            // A batch's ends are `nf_tables` framing; on a route socket the
            // same numbers are a new link and a deleted one.
            if self.protocol == Protocol::Netfilter && BATCH.contains(&message_type) {
                continue;
            }
            self.answer(message_type, flags, header);
        }
        Ok(())
    }

    fn recv(&mut self) -> io::Result<Vec<u8>> {
        self.replies
            .pop_front()
            .ok_or_else(|| io::ErrorKind::WouldBlock.into())
    }
}

impl FakeWire {
    fn answer(&mut self, message_type: u16, flags: u16, header: Vec<u8>) {
        let mut state = self.kernel.state();
        state.seen.push((self.protocol, message_type));
        let refusal = state
            .refusals
            .iter()
            .find(|(on, refused, _errno)| *on == self.protocol && *refused == message_type)
            .map(|&(_on, _refused, errno)| errno);
        let (tables, links) = (state.tables.clone(), state.links.clone());
        let dropping = state.dropping.clone();
        drop(state);
        if let Some(errno) = refusal {
            self.replies
                .push_back(error(header, NonZeroI32::new(-errno)));
        } else if flags & NLM_F_DUMP == NLM_F_DUMP {
            let listed = match message_type {
                GETCHAIN => chains(&dropping),
                GETTABLE => self.dump(&tables),
                _ => self.dump(&links),
            };
            self.replies.push_back(listed);
            self.replies
                .push_back(bytes(NetlinkMessage::<RouteNetlinkMessage>::new(
                    NetlinkHeader::default(),
                    NetlinkPayload::Done(DoneMessage::default()),
                )));
        } else if flags & NLM_F_ACK != 0 {
            self.replies.push_back(error(header, None));
        } else {
            self.replies
                .push_back(bytes(NetlinkMessage::from(RouteNetlinkMessage::NewLink(
                    link("fake"),
                ))));
        }
    }

    /// One datagram holding every name, as this protocol lists it.
    fn dump(&self, names: &[String]) -> Vec<u8> {
        names
            .iter()
            .flat_map(|name| match self.protocol {
                Protocol::Route => bytes(NetlinkMessage::from(RouteNetlinkMessage::NewLink(link(
                    name,
                )))),
                Protocol::Netfilter => bytes(NetlinkMessage::from(NetfilterMessage::new(
                    NetfilterHeader::new(NetfilterProtoFamily::Inet, 0, 0),
                    NfTablesMessage::NewTable(TableMessage {
                        attributes: vec![TableAttribute::Name(name.clone())],
                    }),
                ))),
            })
            .collect()
    }
}

/// One datagram listing each of `dropping` as a forward base chain whose
/// policy drops, as the kernel lists chains.
fn chains(dropping: &[(NetfilterProtoFamily, String, String)]) -> Vec<u8> {
    dropping
        .iter()
        .flat_map(|(family, table, chain)| {
            let attributes = vec![
                ChainAttribute::Table(table.clone()),
                ChainAttribute::Name(chain.clone()),
                ChainAttribute::Hook(vec![
                    Hook::Number(HookNumber::Inet(InetHookNumber::Forward)),
                    Hook::Priority(0),
                ]),
                ChainAttribute::Policy(DROP_POLICY),
            ];
            bytes(NetlinkMessage::from(NetfilterMessage::new(
                NetfilterHeader::new(*family, 0, 0),
                NfTablesMessage::NewChain(ChainMessage { attributes }),
            )))
        })
        .collect()
}

/// A link named `name`, numbered [`INDEX`].
fn link(name: &str) -> LinkMessage {
    let mut link = LinkMessage::default();
    link.header.index = INDEX;
    link.attributes = vec![LinkAttribute::IfName(name.to_owned())];
    link
}

/// An acknowledgement of the request whose header is `header`, or, with a
/// `code`, its refusal.
fn error(header: Vec<u8>, code: Option<NonZeroI32>) -> Vec<u8> {
    let mut answer = ErrorMessage::default();
    answer.code = code;
    answer.header = header;
    bytes(NetlinkMessage::<RouteNetlinkMessage>::new(
        NetlinkHeader::default(),
        NetlinkPayload::Error(answer),
    ))
}

/// `message`, finished and serialized.
pub(super) fn bytes<I: NetlinkSerializable>(mut message: NetlinkMessage<I>) -> Vec<u8> {
    message.finalize();
    let mut buffer = vec![0; message.buffer_len()];
    message.serialize(&mut buffer);
    buffer
}
