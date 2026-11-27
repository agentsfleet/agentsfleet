//! An inner environment's init: PID 1 of a process namespace of its own, in
//! a mount namespace of its own, reaping every orphan inside until it is
//! killed.
//!
//! ```text
//!   this thread: unshare(PID) ─► fork ─► restore its next children to ours
//!   init:        unshare(MOUNT) ─► / downstream ─► /proc of the new namespace
//!                ─► ready ─► close every descriptor it inherited ─► reap forever
//! ```

use std::io::{self, Read as _};
use std::os::fd::{AsFd as _, OwnedFd};

use rustix::mount::{MountFlags, MountPropagationFlags, mount, mount_change};
use rustix::process::{Pid, PidfdFlags, WaitId, WaitIdOptions, WaitOptions};
use rustix::thread::{ThreadNameSpaceType, UnshareFlags};

use crate::error::{Result, unconfined};

/// Why an init that ended before it said it was ready is refused.
const DETAIL_UNBUILT: &str = "the inner environment's init ended before it was ready";
/// What the init writes once its namespaces stand.
const READY: [u8; 1] = [1];

/// Starts an init and answers its process descriptor once its namespaces
/// stand. The calling thread's next children are born where they were
/// before, whatever the start answers.
///
/// # Errors
/// A namespace or the fork was refused, or the init ended before it was
/// ready; it is reaped then.
pub(super) fn start() -> Result<OwnedFd> {
    let ours = rustix::process::pidfd_open(rustix::process::getpid(), PidfdFlags::empty())?;
    // Back to our own namespace first: only from there may a thread make a
    // new one.
    rustix::thread::move_into_thread_name_spaces(ours.as_fd(), ThreadNameSpaceType::PROCESS_ID)?;
    let (mut ready, said) = io::pipe()?;
    // SAFETY: a new process namespace changes only which namespace this
    // thread's next children are born in; no descriptor table or file-system
    // state is unshared.
    unsafe { rustix::thread::unshare_unsafe(UnshareFlags::NEWPID) }?;
    // SAFETY: the child makes only system calls until it ends, as the child
    // of a process with other threads must, and never returns from `run`.
    let child = unsafe { libc::fork() };
    if child == 0 {
        run(&said);
    }
    let forked = io::Error::last_os_error();
    rustix::thread::move_into_thread_name_spaces(ours.as_fd(), ThreadNameSpaceType::PROCESS_ID)?;
    let init = Pid::from_raw(child).ok_or(forked)?;
    let init = rustix::process::pidfd_open(init, PidfdFlags::empty())?;
    drop(said);
    let mut heard = [0; 1];
    if ready.read(&mut heard)? == READY.len() {
        return Ok(init);
    }
    rustix::process::waitid(WaitId::PidFd(init.as_fd()), WaitIdOptions::EXITED)?;
    Err(unconfined(DETAIL_UNBUILT))
}

/// The init itself: its namespaces, the word on `ready`, then reaping.
fn run(ready: &io::PipeWriter) -> ! {
    let built = isolate();
    if built.is_ok() {
        let _unheard = rustix::io::write(ready, &READY);
    }
    // SAFETY: closes every descriptor, standard ones included, so no pipe,
    // socket or terminal of the executor's stays open in a process that
    // reads and writes none; nothing here uses one again.
    unsafe { libc::syscall(libc::SYS_close_range, 0, u32::MAX, 0) };
    if built.is_err() {
        // SAFETY: `_exit` ends the child at once, as a forked one must.
        unsafe { libc::_exit(1) };
    }
    reap()
}

/// A mount namespace of its own whose `/proc` shows the new process
/// namespace alone, nothing it mounts reaching the executor's: downstream of
/// the executor's, so a mount the executor swaps in, a new hosts file,
/// reaches it.
fn isolate() -> rustix::io::Result<()> {
    // SAFETY: this process has one thread, so it shares its file-system
    // state with no other.
    unsafe { rustix::thread::unshare_unsafe(UnshareFlags::NEWNS) }?;
    mount_change(
        c"/",
        MountPropagationFlags::REC | MountPropagationFlags::DOWNSTREAM,
    )?;
    let quiet = MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC;
    mount(c"proc", c"/proc", c"proc", quiet, None)
}

/// Reaps every child the kernel hands the namespace's init, forever.
/// `SIGCHLD` is blocked, so one that arrives between a reap and the wait
/// stays pending and wakes it.
fn reap() -> ! {
    // SAFETY: a signal set is plain integers, so all-zero bytes are one.
    let mut children: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: the set lives on this stack for the call, which fills it.
    unsafe { libc::sigemptyset(&raw mut children) };
    // SAFETY: as above.
    unsafe { libc::sigaddset(&raw mut children, libc::SIGCHLD) };
    // SAFETY: the set lives on this stack for the call; a null old mask asks
    // for no copy of it.
    unsafe { libc::sigprocmask(libc::SIG_BLOCK, &raw const children, std::ptr::null_mut()) };
    loop {
        while let Ok(Some(_reaped)) = rustix::process::waitpid(None, WaitOptions::NOHANG) {}
        // SAFETY: as above; a null record asks for no details of the signal.
        unsafe { libc::sigwaitinfo(&raw const children, std::ptr::null_mut()) };
    }
}
