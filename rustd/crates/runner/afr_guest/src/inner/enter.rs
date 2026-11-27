//! What a tenant process does between fork and exec to enter the inner
//! environment, with everything it needs made before the fork.
//!
//! The child of a process with other threads may only make system calls
//! until it execs, so each step below is a system call on a fixed path or on
//! what the parent already opened, built or compiled, and a refusal is a raw
//! error, never a message.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd as _, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::sync::Arc;

use landlock::RulesetCreated;
use rustix::fs::{Gid, Uid};
use rustix::thread::ThreadNameSpaceType;
use seccompiler::BpfProgram;

use super::files::FilesAs;
use super::oom;
use crate::harden::HIGH_NUMBERS;

/// One tenant process's way in.
pub(super) struct Entering {
    /// The environment's init, by its process descriptor: its mount
    /// namespace is joined through it.
    init: OwnedFd,
    /// Where the process works, as the kernel reads a path.
    cwd: CString,
    user: (Uid, Gid),
    /// Enforced once; built for this process alone.
    rules: Option<RulesetCreated>,
    seccomp: Arc<[BpfProgram; 2]>,
}

impl Entering {
    /// The way in to `init`'s environment for a process working in `cwd` as
    /// `user`, confined by `rules` and `seccomp`.
    ///
    /// # Errors
    /// `cwd` holds a NUL byte, which no path the kernel reads does.
    pub(super) fn new(
        init: OwnedFd,
        cwd: &Path,
        user: (Uid, Gid),
        rules: RulesetCreated,
        seccomp: &Arc<[BpfProgram; 2]>,
    ) -> io::Result<Self> {
        Ok(Self {
            init,
            cwd: CString::new(cwd.as_os_str().as_bytes())?,
            user,
            rules: Some(rules),
            seccomp: Arc::clone(seccomp),
        })
    }

    /// Joins the environment's mount namespace, works in its directory, is
    /// the out-of-memory killer's to pick again, becomes the guest user with
    /// no capability left, and confines itself:
    /// `no_new_privs`, Landlock, then seccomp. Runs in the child between
    /// fork and exec.
    ///
    /// # Errors
    /// A step was refused; the spawn fails and nothing runs.
    pub(super) fn enter(&mut self) -> io::Result<()> {
        // Joining a mount namespace resets the working directory to its
        // root, so the directory is entered again inside.
        rustix::thread::move_into_thread_name_spaces(
            self.init.as_fd(),
            ThreadNameSpaceType::MOUNT,
        )?;
        rustix::process::chdir(self.cwd.as_c_str())?;
        // As user 0 with every capability: the file is user 0's, and the
        // write raises the floor the tenant could lower it to.
        FilesAs::root(self.user, oom::killable)?;
        let (uid, gid) = self.user;
        rustix::thread::set_thread_groups(&[])?;
        rustix::thread::set_thread_res_gid(gid, gid, gid)?;
        rustix::thread::set_thread_res_uid(uid, uid, uid)?;
        rustix::thread::set_no_new_privs(true)?;
        let rules = self.rules.take().ok_or_else(refused)?;
        rules.restrict_self().map_err(|_refused| refused())?;
        for program in self.seccomp.iter() {
            seccompiler::apply_filter(program).map_err(|_refused| refused())?;
        }
        seccompiler::apply_filter(&HIGH_NUMBERS).map_err(|_refused| refused())?;
        let held = rustix::thread::capabilities(None)?;
        if held.effective.is_empty() && held.permitted.is_empty() {
            Ok(())
        } else {
            Err(refused())
        }
    }
}

/// A step of confinement refused, as an error made without allocating.
fn refused() -> io::Error {
    io::Error::from(io::ErrorKind::PermissionDenied)
}
