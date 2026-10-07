//! Where a started process runs, and what its ending means there.
//!
//! In a sandbox with a tenant leaf, the engine opens the leaf's `cgroup.procs`
//! and `memory.events` before the sandbox starts and hands both in as a
//! [`Tenant`]. Each process writes itself into the leaf between fork and exec,
//! through that descriptor, so the kernel checks the move against the engine's
//! open-time credentials and no cgroup file system is mounted inside. The
//! leaf's memory limit sits below the sandbox's, so the kernel's out-of-memory
//! killer picks a tenant process and never the executor; the leaf's `oom_kill`
//! count rising is how such a kill is told from any other `SIGKILL`.

use std::fmt;
use std::fs::File;
use std::io::{self, Write as _};
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::OFlags;
use rustix::io::{Errno, FdFlags};
use rustix::process::Signal;

use crate::api::Ending;
use crate::error::{self, Result};

/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// The `memory.events` key counting processes the kernel killed for memory.
const OOM_KILL: &str = "oom_kill";
/// Room for the whole of `memory.events`: six short lines.
const EVENTS_MAX_BYTES: usize = 256;
/// The signal the out-of-memory killer sends.
const SIGKILL: i32 = Signal::KILL.as_raw();

/// Where the processes a session starts are placed.
pub(crate) trait Placement: Send + Sync + fmt::Debug {
    /// Refuses, before anything is started, when no process could be placed.
    fn check(&self) -> Result<()>;

    /// Places the calling process. Runs in the child between fork and exec,
    /// so an implementation makes one async-signal-safe system call at most
    /// and allocates nothing.
    fn enter(&self) -> io::Result<()>;

    /// `ending` as the caller reads it.
    fn judge(&self, ending: Ending) -> Ending;
}

/// Processes stay where the executor runs: an engine with no tenant leaf.
#[derive(Debug)]
pub(crate) struct Inherit;

impl Placement for Inherit {
    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn enter(&self) -> io::Result<()> {
        Ok(())
    }

    fn judge(&self, ending: Ending) -> Ending {
        ending
    }
}

/// The tenant leaf, reached through two descriptors the engine opened.
#[derive(Debug)]
pub struct Tenant {
    procs: File,
    events: File,
    /// The leaf's `oom_kill` count when last read; it only rises.
    seen: AtomicU64,
}

impl Tenant {
    /// The leaf whose `cgroup.procs` is open for writing as `procs` and whose
    /// `memory.events` is open for reading as `events`. Both become
    /// close-on-exec here, so no process started inherits either.
    ///
    /// # Errors
    /// A descriptor refuses close-on-exec, `procs` is not open for writing, or
    /// `events` does not read as `memory.events`.
    pub fn new(procs: OwnedFd, events: OwnedFd) -> Result<Self> {
        let (procs, events) = (File::from(procs), File::from(events));
        let seen = [&procs, &events]
            .into_iter()
            .try_for_each(|held| rustix::io::fcntl_setfd(held, FdFlags::CLOEXEC))
            .map_err(io::Error::from)
            .and_then(|()| writable(&procs))
            .and_then(|()| oom_kills(&events))
            .map_err(error::tenant_unavailable)?;
        Ok(Self {
            procs,
            events,
            seen: AtomicU64::new(seen),
        })
    }
}

impl Placement for Tenant {
    fn check(&self) -> Result<()> {
        writable(&self.procs).map_err(error::tenant_unavailable)
    }

    fn enter(&self) -> io::Result<()> {
        // One `write` system call: `File` buffers nothing.
        (&self.procs).write(SELF).map(drop)
    }

    fn judge(&self, ending: Ending) -> Ending {
        let killed = ending == Ending::Signaled(SIGKILL);
        match killed.then(|| oom_kills(&self.events)) {
            Some(Ok(now)) if now > self.seen.fetch_max(now, Ordering::Relaxed) => {
                Ending::OutOfMemory
            }
            _unchanged => ending,
        }
    }
}

/// Refuses when `placement` cannot place a process, then has every process
/// `command` starts place itself before it execs. A move that fails fails the
/// spawn, so nothing ever runs unplaced.
pub(super) fn place(
    command: &mut std::process::Command,
    placement: &Arc<dyn Placement>,
) -> Result<()> {
    use std::os::unix::process::CommandExt as _;

    placement.check()?;
    let placed = Arc::clone(placement);
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound. `Placement::enter` makes one `write`
    // on a descriptor opened before the fork, or none, and allocates nothing;
    // the `Arc` is only dereferenced there, its count changing in the parent
    // alone.
    unsafe { command.pre_exec(move || placed.enter()) };
    Ok(())
}

/// Whether `fd` is open, and open for writing.
fn writable(fd: impl std::os::fd::AsFd) -> io::Result<()> {
    let mode = rustix::fs::fcntl_getfl(fd)? & OFlags::RWMODE;
    (mode != OFlags::RDONLY)
        .then_some(())
        .ok_or_else(|| io::Error::from(Errno::BADF))
}

/// The leaf's `oom_kill` count, read whole from the start of `memory.events`.
fn oom_kills(events: &File) -> io::Result<u64> {
    let mut buffer = [0; EVENTS_MAX_BYTES];
    let read = events.read_at(&mut buffer, 0)?;
    oom_kill_count(buffer.get(..read).unwrap_or_default())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
}

/// The `oom_kill` count in a `memory.events` rendering.
fn oom_kill_count(events: &[u8]) -> Option<u64> {
    std::str::from_utf8(events)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix(OOM_KILL)?.strip_prefix(' ')?.parse().ok())
}

#[cfg(test)]
#[path = "tenant/tests.rs"]
mod tests;
