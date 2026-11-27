//! The inner environment a tenant process runs in, which the guest agent
//! builds and stands outside.
//!
//! ```text
//!   guest agent (outside) ─► entry(cwd), on the forking thread ─► fork
//!                          child: the entry's steps ─► exec, inside
//!                       ─► leave(), on the forking thread, fork done or not
//!   renew ─► every process inside ends ─► workspace emptied ─► built again
//!   hosts ─► the hosts file every process inside reads, replaced whole
//! ```
//!
//! An engine whose guest agent is the sandbox's root service hands one in;
//! every other guest agent's processes run where it does.

use std::fmt;
use std::io;
use std::path::Path;

use af_guest_wire::error::{self, Result};

/// What a started process runs between fork and exec to enter the inner
/// environment: system calls on what was opened before the fork, nothing
/// allocated.
pub type Entry = Box<dyn FnMut() -> io::Result<()> + Send + Sync>;

/// An environment of its own every tenant process runs in, which its
/// guest agent can end and build again while the sandbox runs on.
pub trait InnerEnvironment: Send + Sync + fmt::Debug {
    /// Readies the calling thread to start one process inside, working in
    /// `cwd`, and hands back what that process runs between fork and exec.
    /// Called on the thread that forks the process, just before the fork.
    ///
    /// # Errors
    /// The environment is being built again, or cannot be entered.
    fn entry(&self, cwd: &Path) -> io::Result<Entry>;

    /// Puts the calling thread back as it was before [`Self::entry`] readied
    /// it, so what it starts next is born outside again. Called on the
    /// forking thread once the fork is done, whether or not it succeeded:
    /// a thread left readied starts no thread of its own process.
    ///
    /// # Errors
    /// The thread could not be put back.
    fn leave(&self) -> io::Result<()>;

    /// Ends every process inside, empties the workspace and builds the
    /// environment again; returns once no process of the old one remains.
    /// Blocks, so it runs off the runtime's own threads.
    ///
    /// # Errors
    /// The old environment would not end, or the new one would not build.
    fn renew(&self) -> io::Result<()>;

    /// Replaces the hosts file every process inside resolves names through,
    /// and the one the next environment is built with, by `file`, whole and
    /// at once: no lookup reads a part of either. Blocks, as
    /// [`InnerEnvironment::renew`] does.
    ///
    /// # Errors
    /// The file could not be written or put in place; the old one stays.
    fn hosts(&self, file: &str) -> io::Result<()>;
}

/// The event a forking thread that could not be put back is logged under.
const EVENT_NOT_LEFT: &str = "inner_environment_not_left";

/// The forking thread, readied to start one process inside an inner
/// environment: dropped once the fork is done, it puts the thread back.
#[derive(Debug)]
pub(super) struct Readied<'a>(Option<&'a dyn InnerEnvironment>);

impl Drop for Readied<'_> {
    fn drop(&mut self) {
        if let Some(inner) = self.0
            && let Err(error) = inner.leave()
        {
            let (reason, event) = (error.to_string(), EVENT_NOT_LEFT);
            tracing::error!(reason, event);
        }
    }
}

/// Has the process `command` starts enter `inner`, when there is one, before
/// it execs, and hands back the readied thread, held until the fork is done.
/// A process that cannot enter fails its spawn, so nothing runs outside the
/// environment it was meant for.
pub(super) fn enter<'a>(
    command: &mut std::process::Command,
    inner: Option<&'a dyn InnerEnvironment>,
    cwd: &Path,
) -> Result<Readied<'a>> {
    use std::os::unix::process::CommandExt as _;

    let Some(inner) = inner else {
        return Ok(Readied(None));
    };
    let entry = inner.entry(cwd).map_err(error::inner_unavailable)?;
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound; an `Entry` makes system calls on
    // what `InnerEnvironment::entry` opened before the fork and allocates
    // nothing.
    unsafe { command.pre_exec(entry) };
    Ok(Readied(Some(inner)))
}
