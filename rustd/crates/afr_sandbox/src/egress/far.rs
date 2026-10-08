//! What the kernel lane proves egress against (`test-util`, Linux, root).
//!
//! The egress objects in the host's namespace, a leftover a killed run would
//! leave, and a host past the sandbox's link to reach or fail to reach.
//!
//! Every object is made with this module's own netlink, so the lane runs no
//! `nft` or `ip` program either.

use std::fs;
use std::io::{self, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::os::fd::{AsFd as _, OwnedFd};
use std::path::Path;
use std::time::Duration;

use super::kernel::{Host, Kernel as _};
use super::slot::Slot;
use super::{FORWARDING, IP_FORWARD, OWN_NAMESPACE, forwards, link, probe, rules};
use crate::error::{Error, Result, Step, netlink};
use crate::network::Allowlist;

/// The far host's link: its host end, and the end in its own namespace.
const FAR_LINK: &str = "afx0";
const FAR_PEER: &str = "afy0";
/// The far network: the host end, an address the far host answers on, and a
/// second one it answers on too, which an allowlist leaves out.
pub const FAR_HOST_SIDE: Ipv4Addr = Ipv4Addr::new(10, 70, 0, 1);
/// See [`FAR_HOST_SIDE`].
pub const FAR_LISTED: Ipv4Addr = Ipv4Addr::new(10, 70, 0, 2);
/// See [`FAR_HOST_SIDE`].
pub const FAR_UNLISTED: Ipv4Addr = Ipv4Addr::new(10, 70, 0, 3);
/// The far network's prefix: the three addresses above and room to spare.
const FAR_PREFIX_LEN: u8 = 29;
/// The port the far host answers on, and the resolver port it answers on too,
/// which every sandbox's rules close.
pub const FAR_PORT: u16 = 8443;
/// See [`FAR_PORT`].
pub const DNS_PORT: u16 = rules::DNS_PORT;
/// What the far host says to every connection.
pub const FAR_GREETING: &str = "far";
/// Why a slot that does not exist cannot be left behind.
const NO_SUCH_SLOT: &str = "no such slot";
/// How long a connection from the far host waits before it counts as dropped.
const FAR_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// The table the probe trial leaves a dropping forward chain in; not an egress
/// table's name, so no sweep removes it.
const PROBE_TRIAL_TABLE: &str = "afprobetrial";

/// Whether this host forwards IPv4, as the probe reads it: an allowlisted
/// sandbox's traffic is forwarded from its link, so the egress trials need it.
#[must_use]
pub fn forwarding_on() -> bool {
    forwards(Path::new(IP_FORWARD)).unwrap_or(false)
}

/// Every egress table and link in the host's namespace, by name.
///
/// # Errors
/// The kernel would not list them.
pub fn objects() -> Result<Vec<String>> {
    let (tables, links) = super::objects(&Host)?;
    Ok(tables.into_iter().chain(links).collect())
}

/// Leaves slot `index`'s table and link in the host's namespace, held by no
/// scope, as a run killed mid-lease leaves them.
///
/// # Errors
/// No such slot, or the kernel refused.
pub fn leave(index: u8) -> Result<()> {
    let slot = Slot::new(index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, NO_SUCH_SLOT))?;
    // Unowned, as a runner built before tables were owned leaves it: an owned
    // table goes with the socket that made it.
    Host.netfilter()?
        .acknowledged(rules::install_unowned(slot, &[]))?;
    // Both ends in the host's namespace, so the pair outlives every process.
    let own = std::fs::File::open(OWN_NAMESPACE)?;
    link::join(&mut Host.route()?, slot, own.as_fd())?;
    Ok(())
}

/// Deletes the table named `table` from a socket that does not own it, the
/// two ways a host tool would: by name, then as `nft flush ruleset` narrowed
/// to that name, and returns the kernel's answer to each.
///
/// # Errors
/// No netfilter socket opened.
pub fn delete_from_elsewhere(table: &str) -> Result<[io::Result<()>; 2]> {
    let mut netfilter = Host.netfilter()?;
    let by_name = rules::remove(&mut netfilter, table);
    let flushed = netfilter.acknowledged(rules::flush_named(table));
    Ok([by_name, flushed])
}

/// Runs the probe in a network namespace of its own that forwards IPv4, first
/// beside a forward chain that drops by policy and then with that chain gone,
/// and answers whether each run reported enforcement.
///
/// # Errors
/// The namespace, its forwarding or the dropping chain could not be made.
pub fn probe_beside_a_dropping_forward_chain() -> Result<[bool; 2]> {
    let namespace = Host.fresh_namespace()?;
    Host.inside(namespace.as_fd(), || {
        fs::write(IP_FORWARD, FORWARDING)?;
        let mut netfilter = Host.netfilter()?;
        netfilter.acknowledged(rules::dropping_forward_table(PROBE_TRIAL_TABLE))?;
        let beside = probe(&Host, Path::new(IP_FORWARD)).is_ok();
        rules::remove(&mut netfilter, PROBE_TRIAL_TABLE)?;
        let without = probe(&Host, Path::new(IP_FORWARD)).is_ok();
        Ok::<_, Error>([beside, without])
    })
}

/// A host past the sandbox's link, in a namespace of its own.
///
/// Joined to the host, it answers [`FAR_GREETING`] over TCP and UDP on
/// [`FAR_PORT`] and [`DNS_PORT`] at both [`FAR_LISTED`] and [`FAR_UNLISTED`],
/// and routes back through the host. Its link is removed when it drops; its
/// listening threads end with the process.
#[derive(Debug)]
pub struct Far {
    namespace: OwnedFd,
}

impl Far {
    /// Builds the far host and starts it answering.
    ///
    /// # Errors
    /// The kernel refused a step.
    pub fn start() -> Result<Self> {
        let namespace = Host.fresh_namespace()?;
        let mut route = Host.route()?;
        let _stale = remove_link(FAR_LINK);
        route
            .acknowledged(vec![far_pair(namespace.as_fd())])
            .map_err(netlink(Step::Far))?;
        configure(&mut route, FAR_LINK, &[FAR_HOST_SIDE]).map_err(netlink(Step::Far))?;
        Host.inside(namespace.as_fd(), || {
            let mut route = Host.route()?;
            configure(&mut route, FAR_PEER, &[FAR_LISTED, FAR_UNLISTED])?;
            let index = link::index_named(&mut route, FAR_PEER)?;
            route.acknowledged(vec![link::default_route(index, FAR_HOST_SIDE)])?;
            [FAR_PORT, DNS_PORT].into_iter().try_for_each(listen)?;
            [FAR_PORT, DNS_PORT]
                .into_iter()
                .try_for_each(answer_datagrams)
        })
        .map_err(netlink(Step::Far))?;
        Ok(Self { namespace })
    }

    /// Connects from the far host to `address` and returns what came back:
    /// how the lane proves a connection into a sandbox is dropped.
    ///
    /// # Errors
    /// The connection failed or timed out.
    pub fn connect_from(&self, address: SocketAddr) -> io::Result<String> {
        Host.inside(self.namespace.as_fd(), || {
            let mut stream = TcpStream::connect_timeout(&address, FAR_CONNECT_TIMEOUT)?;
            stream.set_read_timeout(Some(FAR_CONNECT_TIMEOUT))?;
            let mut said = String::new();
            stream.read_to_string(&mut said)?;
            Ok(said)
        })
    }

    /// An allowlist naming the far host's listed address.
    ///
    /// # Errors
    /// Never, for one address.
    pub fn allowlist(name: &str) -> Result<Allowlist> {
        Allowlist::new(vec![(name.to_owned(), FAR_LISTED)])
    }
}

impl Drop for Far {
    fn drop(&mut self) {
        let _gone = remove_link(FAR_LINK);
    }
}

/// The far pair: [`FAR_LINK`] here, [`FAR_PEER`] in `netns`.
fn far_pair(netns: std::os::fd::BorrowedFd<'_>) -> link::Message {
    link::named_pair(FAR_LINK, FAR_PEER, netns)
}

/// Gives the link named `name` each of `addresses` and brings it up.
fn configure(
    route: &mut super::netlink::Netlink<impl super::netlink::Wire>,
    name: &str,
    addresses: &[Ipv4Addr],
) -> io::Result<()> {
    let index = link::index_named(route, name)?;
    let mut steps: Vec<_> = addresses
        .iter()
        .map(|&address| link::address_within(index, address, FAR_PREFIX_LEN))
        .collect();
    steps.push(link::up(index));
    route.acknowledged(steps)
}

/// Answers every connection on `port` with the greeting, on a thread of its
/// own in the calling thread's namespace.
fn listen(port: u16) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?;
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _said = stream.write_all(FAR_GREETING.as_bytes());
        }
    });
    Ok(())
}

/// Answers every datagram on `port` with the greeting, on a thread of its own
/// in the calling thread's namespace.
fn answer_datagrams(port: u16) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
    std::thread::spawn(move || {
        let mut buffer = [0; 64];
        while let Ok((_read, from)) = socket.recv_from(&mut buffer) {
            let _said = socket.send_to(FAR_GREETING.as_bytes(), from);
        }
    });
    Ok(())
}

/// Removes the link named `name`, whatever namespace this is.
fn remove_link(name: &str) -> io::Result<()> {
    link::remove(&mut Host.route()?, name)
}
