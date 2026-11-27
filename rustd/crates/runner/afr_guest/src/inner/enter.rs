//! What a tenant process does between fork and exec to enter the inner
//! environment, with everything it needs made before the fork.
//!
//! The child of a process with other threads may only make system calls
//! until it execs, so each step below is a system call on a fixed path or on
//! what the parent already opened, built or compiled, and a refusal is a raw
//! error, never a message.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd as _, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use rustix::fs::{Gid, Uid};
use rustix::thread::ThreadNameSpaceType;

use super::files::FilesAs;
use super::oom;
use crate::harden::Applying;

/// The namespaces a tenant process joins itself: where it reads files, and
/// where it makes and finds System V objects and POSIX message queues. Its
/// process namespace it is born into.
const JOINED: ThreadNameSpaceType =
    ThreadNameSpaceType::MOUNT.union(ThreadNameSpaceType::INTER_PROCESS_COMMUNICATION);

/// Moves the calling thread into the [`JOINED`] namespaces of `init`.
///
/// # Errors
/// The kernel refused: the caller lacks the capability, or `init` ended.
pub(super) fn join(init: BorrowedFd<'_>) -> io::Result<()> {
    Ok(rustix::thread::move_into_thread_name_spaces(init, JOINED)?)
}

/// One tenant process's way in.
pub(super) struct Entering {
    /// The environment's init, by its process descriptor: its mount and
    /// inter-process communication namespaces are joined through it.
    init: OwnedFd,
    /// Where the process works, as the kernel reads a path.
    cwd: CString,
    user: (Uid, Gid),
    /// The confinement the executor chose, prepared for this process alone.
    confine: Applying,
}

impl Entering {
    /// The way in to `init`'s environment for a process working in `cwd` as
    /// `user`, which then applies `confine` to itself.
    ///
    /// # Errors
    /// `cwd` holds a NUL byte, which no path the kernel reads does.
    pub(super) fn new(
        init: OwnedFd,
        cwd: &Path,
        user: (Uid, Gid),
        confine: Applying,
    ) -> io::Result<Self> {
        Ok(Self {
            init,
            cwd: CString::new(cwd.as_os_str().as_bytes())?,
            user,
            confine,
        })
    }

    /// Joins the environment's mount and inter-process communication
    /// namespaces, works in its directory, is the out-of-memory killer's to
    /// pick again, becomes the guest user, and confines itself as the
    /// executor chose. Runs in the child between fork and exec.
    ///
    /// # Errors
    /// A step was refused; the spawn fails and nothing runs.
    pub(super) fn enter(&mut self) -> io::Result<()> {
        // Joining a mount namespace resets the working directory to its
        // root, so the directory is entered again inside.
        join(self.init.as_fd())?;
        rustix::process::chdir(self.cwd.as_c_str())?;
        // As user 0 with every capability: the file is user 0's, and the
        // write raises the floor the tenant could lower it to.
        FilesAs::root(self.user, oom::killable)?;
        let (uid, gid) = self.user;
        rustix::thread::set_thread_groups(&[])?;
        rustix::thread::set_thread_res_gid(gid, gid, gid)?;
        rustix::thread::set_thread_res_uid(uid, uid, uid)?;
        Ok((self.confine)()?)
    }
}
