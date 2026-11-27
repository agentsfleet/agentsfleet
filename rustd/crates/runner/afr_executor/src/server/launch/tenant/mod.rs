//! Where a started process runs, and what its ending means there.
//!
//! In a microVM the guest entry makes a tenant leaf in the guest's own cgroup
//! tree, opens its `cgroup.procs`, `memory.events` and `pids.events` while it
//! is still user 0, and hands all three in as a [`Tenant`]. Each process writes itself into the
//! leaf between fork and exec, through that descriptor, so the kernel checks
//! the move against the open-time credentials, not the tenant's. The executor
//! is never the out-of-memory killer's pick (`afr_guest`), so a guest out of
//! memory loses a tenant process; the leaf's `oom_kill` count, which rises for
//! a kill by any out-of-memory killer, is how such a kill is told from any
//! other `SIGKILL`. A tool's command runs under `/bin/sh -c`, so the process
//! killed is often a child the shell forked, and the shell, still alive,
//! exits with the status it reports that kill under; that exit is judged the
//! same way. The leaf's `pids.events` counts each fork refused at its process
//! cap, which the host reads once a run ends.
//!
//! The leaf is also what a save's pin pause freezes: the guest hands in a
//! [`LeafFreezer`] over it with the three descriptors, and the executor,
//! outside the leaf, freezes every tenant process — a detached one too, as
//! the leaf holds whatever a placed process forks — while it keeps answering.

use std::fmt;
use std::fs::File;
use std::io::{self, Write as _};
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use af_core::error_code;
use rustix::fs::OFlags;
use rustix::io::{Errno, FdFlags};

use crate::api::{Ending, SHELL_KILLED_EXIT, SIGKILL};
use crate::error::{self, Result};

/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// The `memory.events` key counting processes the kernel killed for memory.
const OOM_KILL: &str = "oom_kill";
/// The `pids.events` key counting forks refused at the leaf's `pids.max`.
const PIDS_REFUSED: &str = "max";
/// Room for the whole of `memory.events`, six short lines, and of
/// `pids.events`, two.
const EVENTS_MAX_BYTES: usize = 256;
/// A `memory.events` read failed, so a kill reads as the ending reported.
const EVENT_EVENTS_UNREAD: &str = "executor_memory_events_unread";
/// A failed freeze could not be thawed either.
const EVENT_UNTHAWED: &str = "executor_tenant_unthawed";

/// The state a freeze reaches, and a thaw.
const FROZEN: &str = "frozen";
const THAWED: &str = "thawed";

/// The cgroup freezer over the tenant leaf, handed in by the guest that made it.
///
/// It stops every process in the leaf where it stands and lets them run on;
/// both block until the kernel reports the leaf settled.
pub trait LeafFreezer: Send + Sync + fmt::Debug {
    /// Returns once every process in the leaf is stopped.
    ///
    /// # Errors
    /// The kernel refused the request, or the leaf did not settle.
    fn freeze(&self) -> io::Result<()>;

    /// Returns once no process in the leaf is stopped.
    ///
    /// # Errors
    /// As [`LeafFreezer::freeze`].
    fn thaw(&self) -> io::Result<()>;
}

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

    /// Whether no process placed here still runs. One a call started and
    /// left behind is still the tenant's, and the disk may change under it.
    ///
    /// # Errors
    /// What is placed here could not be read.
    fn idle(&self) -> io::Result<bool>;

    /// The forks refused at this placement's process cap so far.
    ///
    /// # Errors
    /// The count could not be read.
    fn pids_refused(&self) -> io::Result<u64>;

    /// Stops every process placed here, and every one they forked; a freeze
    /// that fails is thawed before it is answered, so it holds nothing. A
    /// placement with no leaf freezes nothing and says so.
    ///
    /// # Errors
    /// Nothing here can be frozen, or the freeze failed.
    fn freeze(&self) -> Result<()> {
        Err(error::no_tenant_leaf())
    }

    /// Lets every process placed here run on.
    ///
    /// # Errors
    /// Nothing here can be frozen, or the thaw failed.
    fn thaw(&self) -> Result<()> {
        Err(error::no_tenant_leaf())
    }
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

    /// It places nothing, so it holds nothing: a session's own calls are all
    /// that is known to run.
    fn idle(&self) -> io::Result<bool> {
        Ok(true)
    }

    /// It caps nothing, so it refuses nothing.
    fn pids_refused(&self) -> io::Result<u64> {
        Ok(0)
    }
}

/// The tenant leaf, reached through three descriptors the engine opened.
#[derive(Debug)]
pub struct Tenant {
    procs: File,
    events: File,
    pids_events: File,
    /// The leaf's `oom_kill` count when last read; it only rises.
    seen: AtomicU64,
    /// The freezer over the leaf.
    freezer: Arc<dyn LeafFreezer>,
}

impl Tenant {
    /// The leaf whose `cgroup.procs` is open for reading and writing as
    /// `procs`, whose `memory.events` is open for reading as `events` and
    /// whose `pids.events` is open for reading as `pids_events`, frozen by
    /// `freezer`. All three become close-on-exec here, so no process started
    /// inherits any.
    ///
    /// # Errors
    /// A descriptor refuses close-on-exec, `procs` is not open for both,
    /// `events` does not read as `memory.events`, or `pids_events` as
    /// `pids.events`.
    pub fn new(
        procs: OwnedFd,
        events: OwnedFd,
        pids_events: OwnedFd,
        freezer: Arc<dyn LeafFreezer>,
    ) -> Result<Self> {
        let (procs, events, pids_events) = (
            File::from(procs),
            File::from(events),
            File::from(pids_events),
        );
        let seen = [&procs, &events, &pids_events]
            .into_iter()
            .try_for_each(|held| rustix::io::fcntl_setfd(held, FdFlags::CLOEXEC))
            .map_err(io::Error::from)
            .and_then(|()| writable(&procs))
            .and_then(|()| emptied(&procs))
            .and_then(|_idle| count(&pids_events, PIDS_REFUSED))
            .and_then(|_refused| count(&events, OOM_KILL))
            .map_err(error::tenant_unavailable)?;
        Ok(Self {
            procs,
            events,
            pids_events,
            seen: AtomicU64::new(seen),
            freezer,
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
        // The process itself killed, or a child its shell forked.
        let killed = matches!(
            ending,
            Ending::Signaled(SIGKILL) | Ending::Exited(SHELL_KILLED_EXIT)
        );
        match killed.then(|| count(&self.events, OOM_KILL)) {
            Some(Ok(now)) if now > self.seen.fetch_max(now, Ordering::Relaxed) => {
                Ending::OutOfMemory
            }
            Some(Err(failure)) => {
                unread(&failure);
                ending
            }
            _unchanged => ending,
        }
    }

    fn idle(&self) -> io::Result<bool> {
        emptied(&self.procs)
    }

    fn pids_refused(&self) -> io::Result<u64> {
        count(&self.pids_events, PIDS_REFUSED)
    }

    fn freeze(&self) -> Result<()> {
        self.freezer.freeze().map_err(|failure| {
            // A freeze that did not settle may still hold some of the leaf.
            if let Err(stuck) = self.freezer.thaw() {
                unthawed(&stuck);
            }
            error::tenant_unfrozen(FROZEN)(failure)
        })
    }

    fn thaw(&self) -> Result<()> {
        self.freezer.thaw().map_err(error::tenant_unfrozen(THAWED))
    }
}

/// Logs a `memory.events` read that failed: the kill it might have named
/// reaches the caller as the ending reported.
fn unread(failure: &io::Error) {
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = failure.to_string();
    let event = EVENT_EVENTS_UNREAD;
    tracing::warn!(error_code, reason, event);
}

/// Logs a thaw that failed after a failed freeze: the leaf may still hold
/// some tenant processes, and the next thaw is what frees them.
fn unthawed(failure: &io::Error) {
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = failure.to_string();
    let event = EVENT_UNTHAWED;
    tracing::warn!(error_code, reason, event);
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

/// Whether the leaf whose `cgroup.procs` is `procs` holds no process. The
/// kernel lists a leaf's live processes from the start of the file, so one
/// byte tells an empty list from any other.
fn emptied(procs: &File) -> io::Result<bool> {
    let mut first = [0; 1];
    Ok(procs.read_at(&mut first, 0)? == 0)
}

/// Whether `fd` is open, and open for writing.
fn writable(fd: impl std::os::fd::AsFd) -> io::Result<()> {
    let mode = rustix::fs::fcntl_getfl(fd)? & OFlags::RWMODE;
    (mode != OFlags::RDONLY)
        .then_some(())
        .ok_or_else(|| io::Error::from(Errno::BADF))
}

/// The leaf's `key` count, read whole from the start of the events file
/// `events`.
fn count(events: &File, key: &str) -> io::Result<u64> {
    let mut buffer = [0; EVENTS_MAX_BYTES];
    let read = events.read_at(&mut buffer, 0)?;
    event_count(buffer.get(..read).unwrap_or_default(), key)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
}

/// The `key` count in an events file's rendering, as `memory.events` and
/// `pids.events` render theirs: one `key count` line each.
fn event_count(events: &[u8], key: &str) -> Option<u64> {
    std::str::from_utf8(events)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(' ')?.parse().ok())
}

#[cfg(test)]
pub(super) mod freeze_tests;
#[cfg(test)]
mod spawn_tests;
#[cfg(test)]
mod tests;
