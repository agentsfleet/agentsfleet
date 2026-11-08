//! The descriptors the sandbox entry inherits, from the engine that opens
//! them to the executor that uses them: the tenant leaf's two files, and the
//! entry's end of the executor's socket pair.
//!
//! The engine opens the leaf's `cgroup.procs` for writing and its
//! `memory.events` for reading, as the runner, before bubblewrap starts.
//! Bubblewrap and the sandbox entry inherit both across their execs, and the
//! entry is told their numbers on its command line; the executor then holds
//! them close-on-exec, so no tenant process does. Opened outside, the procs
//! descriptor carries the runner's credentials and cgroup namespace, which is
//! what lets a process inside move itself into the leaf with no cgroup file
//! system mounted.
//!
//! The socket pair is made close-on-exec, and only the spawned child's copy
//! of the entry's end loses the flag, so no other spawn on the host inherits
//! it. The entry sets the flag again before it hardens, so no tenant process
//! holds the link either.

use std::ffi::OsString;
use std::fs;
use std::os::fd::{FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

use clap::Parser as _;
use rustix::io::FdFlags;

use crate::error::{Result, not_inherited};

#[cfg(any(target_os = "linux", test))]
mod files;

#[cfg(any(target_os = "linux", test))]
pub(crate) use self::files::{Handover, TenantFiles};

/// The flag naming the tenant leaf's `cgroup.procs`.
pub const TENANT_PROCS_FLAG: &str = "--tenant-procs";
/// The flag naming the tenant leaf's `memory.events`.
pub const TENANT_EVENTS_FLAG: &str = "--tenant-events";
/// The flag naming the entry's end of the executor's socket pair.
pub const EXECUTOR_FLAG: &str = "--executor";
/// Where a process finds the descriptors it holds, one entry per number.
const HELD: &str = "/dev/fd";
/// The lowest number an inherited descriptor may have: 0 to 2 are the
/// standard streams.
const FIRST_INHERITED: RawFd = 3;

/// The numbers the sandbox entry inherited the tenant leaf's files under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::Args)]
pub struct TenantDescriptors {
    /// The tenant leaf's `cgroup.procs`, open for writing.
    #[arg(long)]
    pub tenant_procs: RawFd,
    /// The tenant leaf's `memory.events`, open for reading.
    #[arg(long)]
    pub tenant_events: RawFd,
}

/// Every number the sandbox entry inherited a descriptor under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::Args)]
pub struct Inherited {
    /// The tenant leaf's two files.
    #[command(flatten)]
    pub tenant: TenantDescriptors,
    /// The entry's end of the executor's socket pair, whose other end the
    /// runner holds.
    #[arg(long)]
    pub executor: RawFd,
}

/// The sandbox entry's command line, for a caller with no parser of its own.
#[derive(Debug, clap::Parser)]
struct Entry {
    #[command(flatten)]
    inherited: Inherited,
}

impl Inherited {
    /// The descriptors `args` names, its first item being the entry's own
    /// name.
    ///
    /// # Errors
    /// A flag is missing or does not name a number.
    pub fn parse_from(args: impl IntoIterator<Item = OsString>) -> Result<Self, clap::Error> {
        Entry::try_parse_from(args).map(|entry| entry.inherited)
    }

    /// The sandbox entry's flags naming all three.
    #[must_use]
    pub fn arguments(self) -> [OsString; 6] {
        let [procs_flag, procs, events_flag, events] = self.tenant.arguments();
        [
            procs_flag,
            procs,
            events_flag,
            events,
            EXECUTOR_FLAG.into(),
            self.executor.to_string().into(),
        ]
    }

    /// Takes ownership of all three: the tenant leaf the executor places its
    /// processes in, and the link it serves. The link is close-on-exec again
    /// before anything starts a process, so no tenant process inherits it.
    /// The sandbox entry calls this once, before it opens anything or starts
    /// a thread.
    ///
    /// # Errors
    /// A number names a standard stream or a descriptor this process does not
    /// hold, two name one descriptor, or the executor refuses the leaf.
    pub(crate) fn adopt(self) -> Result<(afr_executor::Tenant, UnixStream)> {
        let tenant = [self.tenant.tenant_procs, self.tenant.tenant_events];
        if tenant.contains(&self.executor) || !held(self.executor) {
            return Err(not_inherited(self.executor));
        }
        let leaf = self.tenant.adopt()?;
        // SAFETY: the number names a descriptor this process holds (`/dev/fd`
        // lists it), not a standard stream and neither tenant file. Nothing
        // has taken ownership of it: the engine made the pair for this
        // process alone and named its end once, and adoption runs once.
        let link = unsafe { OwnedFd::from_raw_fd(self.executor) };
        rustix::io::fcntl_setfd(&link, FdFlags::CLOEXEC).map_err(std::io::Error::from)?;
        Ok((leaf, UnixStream::from(link)))
    }
}

impl TenantDescriptors {
    /// The sandbox entry's flags naming both.
    #[must_use]
    pub fn arguments(self) -> [OsString; 4] {
        [
            TENANT_PROCS_FLAG.into(),
            self.tenant_procs.to_string().into(),
            TENANT_EVENTS_FLAG.into(),
            self.tenant_events.to_string().into(),
        ]
    }

    /// Takes ownership of both descriptors, inherited from the engine, as the
    /// executor's tenant leaf. The sandbox entry calls this once, before it
    /// opens anything or starts a thread.
    ///
    /// # Errors
    /// A number names a standard stream or a descriptor this process does not
    /// hold, both name one descriptor, or the executor refuses the pair.
    pub(crate) fn adopt(self) -> Result<afr_executor::Tenant> {
        let numbers = [self.tenant_procs, self.tenant_events];
        let distinct = self.tenant_procs != self.tenant_events;
        if let Some(refused) = numbers.into_iter().find(|&n| !distinct || !held(n)) {
            return Err(not_inherited(refused));
        }
        // SAFETY: each number names a descriptor this process holds (`/dev/fd`
        // lists it), neither is a standard stream, and the two differ. Nothing
        // has taken ownership of either: the engine opened them for this
        // process alone and named each once, and adoption runs once, before
        // the entry opens a file or starts a thread.
        let [procs, events] = numbers.map(|n| unsafe { OwnedFd::from_raw_fd(n) });
        Ok(afr_executor::Tenant::new(procs, events)?)
    }
}

/// Whether this process holds descriptor `n` and it is not a standard stream.
fn held(n: RawFd) -> bool {
    n >= FIRST_INHERITED && fs::symlink_metadata(Path::new(HELD).join(n.to_string())).is_ok()
}

#[cfg(test)]
mod tests;
