//! The inner environment a microVM's tenant processes run in: a process
//! namespace, a mount namespace and an inter-process communication (IPC)
//! namespace of their own, which the guest agent builds and stands outside as
//! the guest's root service.
//!
//! ```text
//!   build  ─► the guest agent is never the out-of-memory killer's pick, and
//!             acts on files as the guest user from now on
//!          ─► init: PID 1 of a new process namespace, in a new mount
//!             namespace whose /proc shows that namespace alone and a new
//!             IPC namespace holding nothing; reaps orphans
//!   entry  ─► the forking thread's next child joins init's process namespace
//!   leave  ─► the forking thread's next children are born in the guest agent's
//!             own process namespace again, where alone it may start a thread
//!          ─► child: init's mount and IPC namespaces, cd, pickable again,
//!             the guest user, no_new_privs, Landlock, seccomp (no namespace
//!             made or joined) ─► exec
//!   renew  ─► SIGKILL init: the kernel ends every process inside, and init
//!             is reaped only once they all are; its IPC namespace goes with
//!             its last process, and every System V object and POSIX queue
//!             in it ─► /workspace, /tmp and /dev/shm emptied ─► build
//!   hosts  ─► as user 0: a new hosts file staged, its bind swapped in beneath
//!             the old one, which goes; the swap reaches init's namespace
//! ```
//!
//! Ending init is what ends the environment, so no tenant process outlives
//! it: a process that left its group, its session or its parent still dies
//! with the namespace's init. The guest agent's own file calls act as the guest
//! user, so what it writes the tenant owns.

use std::fs;
use std::io;
use std::os::fd::{AsFd as _, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use afg_agent::{Entry, InnerEnvironment};
use rustix::fs::{Gid, Uid};
use rustix::process::{PidfdFlags, Signal, WaitId, WaitIdOptions};
use rustix::thread::ThreadNameSpaceType;

use crate::error::{Result, unconfined};
use crate::harden::{Confinement, DEV_SHM, PROC_SELF_STATUS, SANDBOX_TMP, single_threaded};

mod enter;
mod files;
mod init;
mod names;
mod oom;

use self::enter::Entering;
use self::files::{FilesAs, clear};

/// Why a spawn found no environment to enter: a renewal is building the
/// next one.
const DETAIL_RENEWING: &str = "the inner environment is being built again";
/// The trees beside the workspace a tenant process writes and its files
/// outlive it in: the disk's scratch space and the shared-memory `tmpfs`. A
/// pseudo-terminal goes with the process that held it.
const SCRATCH: [&str; 2] = [SANDBOX_TMP, DEV_SHM];

/// A process namespace, a mount namespace and an IPC namespace of their own,
/// which every tenant process runs in as the guest user, confined as a
/// sandbox is.
#[derive(Debug)]
pub struct Namespaced {
    user: (Uid, Gid),
    workspace: PathBuf,
    /// The hosts file every process inside reads, and the directory each new
    /// one is staged in.
    hosts: PathBuf,
    staging: PathBuf,
    /// What every process started inside puts on itself, which the guest agent
    /// chose when it built the environment.
    confinement: Box<dyn Confinement>,
    /// The environment's init, by its process descriptor; none while a
    /// renewal builds the next one.
    init: Mutex<Option<OwnedFd>>,
    /// The guest agent itself, by its process descriptor: whose process
    /// namespace a forking thread's next children go back to.
    ours: OwnedFd,
}

impl Namespaced {
    /// Builds the first inner environment for tenant processes running as
    /// `user`, a user and group identifier, over `workspace`, and has this
    /// process act on files as `user` from now on: every thread made after
    /// inherits that. This process is never the out-of-memory killer's pick
    /// from now on, and each tenant process is pickable again as it enters.
    /// Its hosts file is the mount on `hosts`, and each new one is staged in
    /// `staging`, which no tenant process writes. Every process started
    /// inside applies `confinement` to itself: the guest agent's choice, which
    /// no request reaches.
    ///
    /// # Errors
    /// A second thread already exists, so the file identity would miss it;
    /// the standing or the identity would not change; or the environment
    /// would not build.
    pub fn build(
        user: (u32, u32),
        workspace: &Path,
        hosts: &Path,
        staging: &Path,
        confinement: Box<dyn Confinement>,
    ) -> Result<Self> {
        single_threaded(&fs::read_to_string(PROC_SELF_STATUS)?)?;
        oom::spare(oom::OOM_SCORE_ADJ)?;
        let user = (Uid::from_raw(user.0), Gid::from_raw(user.1));
        rustix::thread::set_thread_groups(&[])?;
        FilesAs::user(user)?;
        let built = Self {
            user,
            workspace: workspace.to_owned(),
            hosts: hosts.to_owned(),
            staging: staging.to_owned(),
            confinement,
            init: Mutex::new(None),
            ours: rustix::process::pidfd_open(rustix::process::getpid(), PidfdFlags::empty())?,
        };
        *built.held() = Some(init::start()?);
        Ok(built)
    }

    fn held(&self) -> MutexGuard<'_, Option<OwnedFd>> {
        // Nothing panics while the lock is held, so a poisoned one still
        // holds a whole value.
        self.init.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl InnerEnvironment for Namespaced {
    fn entry(&self, cwd: &Path) -> io::Result<Entry> {
        let init = self
            .held()
            .as_ref()
            .ok_or_else(|| io::Error::other(unconfined(DETAIL_RENEWING)))?
            .try_clone()?;
        let confine = self.confinement.prepare().map_err(io::Error::other)?;
        let joined = init.try_clone()?;
        let mut entering = Entering::new(init, cwd, self.user, confine)?;
        // Last, so a refusal above leaves the thread as it was. The calling
        // thread's next child is born inside: a process cannot move itself
        // into another process namespace, only its children.
        rustix::thread::move_into_thread_name_spaces(
            joined.as_fd(),
            ThreadNameSpaceType::PROCESS_ID,
        )?;
        Ok(Box::new(move || entering.enter()))
    }

    fn leave(&self) -> io::Result<()> {
        // While its children would be born inside, the kernel refuses this
        // thread a thread of its own process: an export's, or the runtime's.
        Ok(rustix::thread::move_into_thread_name_spaces(
            self.ours.as_fd(),
            ThreadNameSpaceType::PROCESS_ID,
        )?)
    }

    fn renew(&self) -> io::Result<()> {
        // Taken first, so a spawn while the old one ends is refused rather
        // than born into it.
        let old = self.held().take();
        if let Some(init) = old {
            end(&init)?;
        }
        // Nothing the old environment wrote outlives it: what a restore
        // imports lands beside no file of the run before.
        FilesAs::root(self.user, || clear(written(&self.workspace)))?;
        let fresh = init::start().map_err(io::Error::other)?;
        *self.held() = Some(fresh);
        Ok(())
    }

    fn hosts(&self, file: &str) -> io::Result<()> {
        FilesAs::root(self.user, || {
            names::replace(&self.staging, &self.hosts, file)
        })
    }
}

/// Every tree a tenant process writes that outlives it: `workspace`, then
/// [`SCRATCH`].
fn written(workspace: &Path) -> impl Iterator<Item = &Path> {
    std::iter::once(workspace).chain(SCRATCH.into_iter().map(Path::new))
}

/// Kills `init` and waits until it is reaped, which the kernel allows only
/// once every other process in its namespace has ended and been reaped: when
/// this returns, nothing of the old environment runs.
fn end(init: &OwnedFd) -> io::Result<()> {
    match rustix::process::pidfd_send_signal(init, Signal::KILL) {
        // An init that already ended is a zombie, still ours to reap.
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(refused) => return Err(refused.into()),
    }
    rustix::process::waitid(WaitId::PidFd(init.as_fd()), WaitIdOptions::EXITED)?;
    Ok(())
}

#[cfg(test)]
mod tests;
