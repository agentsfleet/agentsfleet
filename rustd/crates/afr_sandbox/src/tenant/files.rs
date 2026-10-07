//! The engine's half: the tenant leaf's files, opened as the runner and let
//! through bubblewrap's exec. Built where the bubblewrap engine is, Linux, and
//! for its tests.

use std::io;
use std::os::fd::{AsRawFd as _, OwnedFd};
use std::path::Path;

use rustix::fs::{Mode, OFlags};
use rustix::io::FdFlags;

use super::TenantDescriptors;
use crate::cgroup::{CGROUP_PROCS, MEMORY_EVENTS};
use crate::error::{Result, cgroup};

/// The mode a control file is created with where the leaf is a plain
/// directory; a cgroup file system publishes its own and ignores it.
const PLAIN_MODE: Mode = Mode::RUSR
    .union(Mode::WUSR)
    .union(Mode::RGRP)
    .union(Mode::ROTH);

/// The tenant leaf's files as the engine opens them, handed across
/// bubblewrap's exec to the sandbox entry.
#[derive(Debug)]
pub(crate) struct TenantFiles {
    pub(super) procs: OwnedFd,
    pub(super) events: OwnedFd,
}

impl TenantFiles {
    /// Opens the leaf's `cgroup.procs` at `procs` for writing and its
    /// `memory.events` at `events` for reading, both close-on-exec.
    pub(crate) fn open(procs: &Path, events: &Path) -> Result<Self> {
        Ok(Self {
            procs: open(procs, OFlags::WRONLY).map_err(cgroup(CGROUP_PROCS))?,
            events: open(events, OFlags::RDONLY).map_err(cgroup(MEMORY_EVENTS))?,
        })
    }

    /// The numbers the sandbox entry will hold them under.
    pub(crate) fn descriptors(&self) -> TenantDescriptors {
        TenantDescriptors {
            tenant_procs: self.procs.as_raw_fd(),
            tenant_events: self.events.as_raw_fd(),
        }
    }

    /// Lets both survive the coming exec. Runs in the child between fork and
    /// exec: two `fcntl` calls, and nothing allocated.
    pub(crate) fn inherit(&self) -> io::Result<()> {
        [&self.procs, &self.events]
            .into_iter()
            .try_for_each(|held| rustix::io::fcntl_setfd(held, FdFlags::empty()))
            .map_err(io::Error::from)
    }
}

/// Opens `path` close-on-exec with `access`. `CREATE` makes the file where the
/// leaf is a plain directory, which proves the engine without root; on a
/// cgroup file system the kernel published it with the leaf and the flag
/// changes nothing.
fn open(path: &Path, access: OFlags) -> io::Result<OwnedFd> {
    let flags = access | OFlags::CREATE | OFlags::CLOEXEC;
    Ok(rustix::fs::open(path, flags, PLAIN_MODE)?)
}
