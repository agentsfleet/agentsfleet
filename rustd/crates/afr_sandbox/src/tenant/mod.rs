//! The tenant leaf's two descriptors, from the engine that opens them to the
//! executor that uses them.
//!
//! The engine opens the leaf's `cgroup.procs` for writing and its
//! `memory.events` for reading, as the runner, before bubblewrap starts.
//! Bubblewrap and the sandbox entry inherit both across their execs, and the
//! entry is told their numbers on its command line; the executor then holds
//! them close-on-exec, so no tenant process does. Opened outside, the procs
//! descriptor carries the runner's credentials and cgroup namespace, which is
//! what lets a process inside move itself into the leaf with no cgroup file
//! system mounted.

use std::ffi::OsString;
use std::fs;
use std::os::fd::{FromRawFd as _, OwnedFd, RawFd};
use std::path::Path;

use clap::Parser as _;

use crate::error::{Result, not_inherited};

#[cfg(any(target_os = "linux", test))]
mod files;

#[cfg(any(target_os = "linux", test))]
pub(crate) use self::files::TenantFiles;

/// The flag naming the tenant leaf's `cgroup.procs`.
pub const TENANT_PROCS_FLAG: &str = "--tenant-procs";
/// The flag naming the tenant leaf's `memory.events`.
pub const TENANT_EVENTS_FLAG: &str = "--tenant-events";
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

/// The sandbox entry's command line, for a caller with no parser of its own.
#[derive(Debug, clap::Parser)]
struct Entry {
    #[command(flatten)]
    tenant: TenantDescriptors,
}

impl TenantDescriptors {
    /// The descriptors `args` names, its first item being the entry's own
    /// name.
    ///
    /// # Errors
    /// A flag is missing or does not name a number.
    pub fn parse_from(
        args: impl IntoIterator<Item = OsString>,
    ) -> std::result::Result<Self, clap::Error> {
        Entry::try_parse_from(args).map(|entry| entry.tenant)
    }

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
