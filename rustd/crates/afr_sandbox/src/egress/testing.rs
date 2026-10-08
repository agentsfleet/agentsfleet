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
    Emitable as _, NLM_F_ACK, NLM_F_DUMP, NetlinkBuffer, NetlinkHeader, NetlinkMessage,
};
use netlink_packet_netfilter::nftables::{
    InetHookNumber, NfTablesMessage, TableAttribute, TableMessage,
};
use netlink_packet_netfilter::{NetfilterHeader, NetfilterMessage, NetfilterProtoFamily};

use self::replies::answered;
pub(super) use self::replies::{ack, chain, done, new_link, refusal, round_trip};
use super::kernel::Kernel;
use super::netlink::{Netlink, Wire, encode, frames};
use super::rules::{DROP, u16_of};

mod replies;

/// The index every link lookup answers.
pub(super) const INDEX: u32 = 7;
/// The route messages a scope sends, by type.
pub(super) const NEWLINK: u16 = libc::RTM_NEWLINK;
pub(super) const DELLINK: u16 = libc::RTM_DELLINK;
pub(super) const GETLINK: u16 = libc::RTM_GETLINK;
pub(super) const SETLINK: u16 = libc::RTM_SETLINK;
pub(super) const NEWADDR: u16 = libc::RTM_NEWADDR;
pub(super) const NEWROUTE: u16 = libc::RTM_NEWROUTE;
/// The `nf_tables` messages a scope sends, by type.
pub(super) const NEWTABLE: u16 = nftables(libc::NFT_MSG_NEWTABLE);
pub(super) const GETTABLE: u16 = nftables(libc::NFT_MSG_GETTABLE);
pub(super) const DELTABLE: u16 = nftables(libc::NFT_MSG_DELTABLE);
pub(super) const NEWCHAIN: u16 = nftables(libc::NFT_MSG_NEWCHAIN);
pub(super) const GETCHAIN: u16 = nftables(libc::NFT_MSG_GETCHAIN);
pub(super) const NEWRULE: u16 = nftables(libc::NFT_MSG_NEWRULE);
pub(super) const NEWSET: u16 = nftables(libc::NFT_MSG_NEWSET);
pub(super) const NEWSETELEM: u16 = nftables(libc::NFT_MSG_NEWSETELEM);
/// An `nf_tables` batch's two ends, which are answered by nothing.
const BATCH: [u16; 2] = [
    u16_of(libc::NFNL_MSG_BATCH_BEGIN),
    u16_of(libc::NFNL_MSG_BATCH_END),
];

/// An `nf_tables` message's type: the subsystem in the high byte, the
/// message in the low.
const fn nftables(message: libc::c_int) -> u16 {
    u16_of(libc::NFNL_SUBSYS_NFTABLES << 8 | message)
}

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
        let header_len = NetlinkHeader::default().buffer_len();
        for frame in frames(datagram) {
            let frame = frame?;
            let buffer = NetlinkBuffer::new(frame);
            let (message_type, flags) = (buffer.message_type(), buffer.flags());
            let header = frame.get(..header_len).unwrap().to_vec();
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
                .push_back(answered(header, NonZeroI32::new(-errno)));
        } else if flags & NLM_F_DUMP == NLM_F_DUMP {
            let listed = match message_type {
                GETCHAIN => chains(&dropping),
                GETTABLE => self.dump(&tables),
                _ => self.dump(&links),
            };
            self.replies.push_back(listed);
            self.replies.push_back(done());
        } else if flags & NLM_F_ACK != 0 {
            self.replies.push_back(answered(header, None));
        } else {
            self.replies.push_back(new_link("fake"));
        }
    }

    /// One datagram holding every name, as this protocol lists it.
    fn dump(&self, names: &[String]) -> Vec<u8> {
        names
            .iter()
            .flat_map(|name| match self.protocol {
                Protocol::Route => new_link(name),
                Protocol::Netfilter => encode(NetlinkMessage::from(NetfilterMessage::new(
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
        .flat_map(|(family, table, name)| {
            let listed = chain(*family, table, name, InetHookNumber::Forward, DROP);
            encode(NetlinkMessage::from(listed))
        })
        .collect()
}
