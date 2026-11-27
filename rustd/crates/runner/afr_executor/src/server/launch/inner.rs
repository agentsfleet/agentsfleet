//! The inner environment a tenant process runs in, which the executor
//! builds and stands outside.
//!
//! ```text
//!   executor (outside) ─► entry(cwd), on the forking thread ─► fork
//!                          child: the entry's steps ─► exec, inside
//!   renew ─► every process inside ends ─► workspace emptied ─► built again
//!   hosts ─► the hosts file every process inside reads, replaced whole
//! ```
//!
//! An engine whose executor is the sandbox's root service hands one in;
//! every other executor's processes run where it does.

use std::fmt;
use std::io;
use std::path::Path;

use crate::error::{self, Result};

/// What a started process runs between fork and exec to enter the inner
/// environment: system calls on what was opened before the fork, nothing
/// allocated.
pub type Entry = Box<dyn FnMut() -> io::Result<()> + Send + Sync>;

/// An environment of its own every tenant process runs in, which its
/// executor can end and build again while the sandbox runs on.
pub trait InnerEnvironment: Send + Sync + fmt::Debug {
    /// Readies the calling thread to start one process inside, working in
    /// `cwd`, and hands back what that process runs between fork and exec.
    /// Called on the thread that forks the process, just before the fork.
    ///
    /// # Errors
    /// The environment is being built again, or cannot be entered.
    fn entry(&self, cwd: &Path) -> io::Result<Entry>;

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

/// Has the process `command` starts enter `inner`, when there is one, before
/// it execs. A process that cannot enter fails its spawn, so nothing runs
/// outside the environment it was meant for.
pub(super) fn enter(
    command: &mut std::process::Command,
    inner: Option<&dyn InnerEnvironment>,
    cwd: &Path,
) -> Result<()> {
    use std::os::unix::process::CommandExt as _;

    let Some(inner) = inner else {
        return Ok(());
    };
    let entry = inner.entry(cwd).map_err(error::inner_unavailable)?;
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound; an `Entry` makes system calls on
    // what `InnerEnvironment::entry` opened before the fork and allocates
    // nothing.
    unsafe { command.pre_exec(entry) };
    Ok(())
}
