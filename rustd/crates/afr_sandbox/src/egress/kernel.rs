//! The kernel a scope is built in: its two netlink protocols, and the way into
//! another network namespace.
//!
//! [`Host`] is the running kernel. Behind the trait, a test states a kernel
//! that acknowledges, answers or refuses each request as it chooses, so every
//! path through a scope's build and removal is proven without root.

use std::fs::File;
use std::io;
use std::os::fd::{BorrowedFd, OwnedFd};

use netlink_sys::Socket;
use netlink_sys::protocols::{NETLINK_NETFILTER, NETLINK_ROUTE};
use rustix::thread::{LinkNameSpaceType, UnshareFlags};

use super::netlink::{Netlink, Wire};

/// The network namespace of the calling thread, as a file another thread can
/// join or a link can be moved into.
const THREAD_NAMESPACE: &str = "/proc/thread-self/ns/net";
/// Why a thread sent into another namespace never came back.
const PANICKED: &str = "the thread working inside a network namespace panicked";

/// Where a scope's requests go.
pub(crate) trait Kernel: Sync {
    /// What its sockets speak over.
    type Wire: Wire;

    /// A route-netlink conversation, in the calling thread's namespace.
    ///
    /// # Errors
    /// The kernel refused the socket.
    fn route(&self) -> io::Result<Netlink<Self::Wire>>;

    /// An `nf_tables` conversation, in the calling thread's namespace.
    ///
    /// # Errors
    /// The kernel refused the socket.
    fn netfilter(&self) -> io::Result<Netlink<Self::Wire>>;

    /// Runs `step` on a thread of its own that has joined `netns`, so every
    /// socket `step` opens speaks to that namespace; the caller's thread never
    /// leaves its own.
    ///
    /// # Errors
    /// The thread could not join `netns`, or `step` failed.
    fn inside<T, E>(
        &self,
        netns: BorrowedFd<'_>,
        step: impl FnOnce() -> Result<T, E> + Send,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<io::Error> + Send;

    /// A network namespace of its own, with nothing in it but loopback, which
    /// lives as long as the returned file.
    ///
    /// # Errors
    /// The kernel refused to make one.
    fn fresh_namespace(&self) -> io::Result<OwnedFd>;
}

/// The running kernel.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Host;

impl Kernel for Host {
    type Wire = Socket;

    fn route(&self) -> io::Result<Netlink<Socket>> {
        Netlink::open(NETLINK_ROUTE)
    }

    fn netfilter(&self) -> io::Result<Netlink<Socket>> {
        Netlink::open(NETLINK_NETFILTER)
    }

    fn inside<T, E>(
        &self,
        netns: BorrowedFd<'_>,
        step: impl FnOnce() -> Result<T, E> + Send,
    ) -> Result<T, E>
    where
        T: Send,
        E: From<io::Error> + Send,
    {
        on_own_thread(move || {
            rustix::thread::move_into_link_name_space(netns, Some(LinkNameSpaceType::Network))
                .map_err(io::Error::from)?;
            step()
        })
    }

    fn fresh_namespace(&self) -> io::Result<OwnedFd> {
        on_own_thread(|| {
            // SAFETY: only the network namespace is unshared, and only on this
            // thread, which exists for this call alone; no file table, file
            // system context or memory the rest of the process relies on
            // changes. The namespace outlives the thread through the file.
            unsafe { rustix::thread::unshare_unsafe(UnshareFlags::NEWNET) }
                .map_err(io::Error::from)?;
            Ok(File::open(THREAD_NAMESPACE)?.into())
        })
    }
}

/// Runs `work` on a thread of its own and returns what it returned: a thread
/// that changes its namespace must not be one the caller goes on using.
fn on_own_thread<T, E>(work: impl FnOnce() -> Result<T, E> + Send) -> Result<T, E>
where
    T: Send,
    E: From<io::Error> + Send,
{
    std::thread::scope(|threads| {
        threads
            .spawn(work)
            .join()
            .unwrap_or_else(|_panicked| Err(io::Error::other(PANICKED).into()))
    })
}

#[cfg(test)]
#[path = "kernel_tests.rs"]
mod tests;
