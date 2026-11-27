//! What the process inside a sandbox does to itself before it reads anything.
//!
//! In order: `no_new_privs`, then Landlock, then seccomp, then a check that no
//! capability survived, which [`Hardened`] owns behind [`Confinement`]. Each
//! step binds only the calling thread and its future children, so [`harden`]
//! refuses to run once a second thread exists — the guest agent's runtime starts
//! after it, never before.

#[cfg(target_os = "linux")]
use std::fs;

use procfs_core::FromRead as _;
use procfs_core::process::Status;

use crate::error::{Result, unconfined};

/// Where the workspace disk appears inside the sandbox: the root the agent
/// confines every path to.
pub const SANDBOX_WORKSPACE: &str = af_guest_wire::WORKSPACE_ROOT;
/// Where POSIX shared memory lives: a private `tmpfs` per sandbox, which
/// Python's `multiprocessing` and Chromium need. Its pages are the guest's
/// own memory, so the memory limit covers it.
pub const DEV_SHM: &str = "/dev/shm";
/// Where the workspace disk's `tmp/` is bound: scratch space that shares the
/// disk's limit, so filling it answers `ENOSPC` rather than taking the
/// run's memory.
pub const SANDBOX_TMP: &str = "/tmp";

/// The process status file the checks read.
#[cfg(target_os = "linux")]
pub(crate) const PROC_SELF_STATUS: &str = "/proc/self/status";
/// The name the kernel's security-module list and the capability report give
/// Landlock.
pub const MECHANISM_LANDLOCK: &str = "landlock";
/// Pseudo-terminals, which `--dev` mounts and an interactive process writes.
const DEV_PTS: &str = "/dev/pts";
/// Why a process that still holds a capability is refused.
pub(crate) const CAPABILITY_SURVIVED: &str = "a capability survived";

/// Trees a sandboxed process may write beneath; everything else is read-only.
///
/// No socket is among them: the guest agent's link is the guest's vsock, which
/// no path names.
pub const WRITABLE: [&str; 4] = [SANDBOX_WORKSPACE, SANDBOX_TMP, DEV_PTS, DEV_SHM];
/// Single devices it may also write: what shells and pseudo-terminals open.
pub const WRITABLE_DEVICES: [&str; 5] = [
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/tty",
    "/dev/ptmx",
];

/// Confines the calling process for good.
///
/// # Errors
/// A second thread already exists, a step is refused, Landlock is not fully
/// enforced, or a capability survived.
#[cfg(target_os = "linux")]
pub fn harden() -> Result<()> {
    single_threaded(&fs::read_to_string(PROC_SELF_STATUS)?)?;
    let mut apply = Hardened::new()?.prepare()?;
    Ok(apply()?)
}

/// A host without Landlock cannot confine a sandbox, so it never serves one.
///
/// # Errors
/// Always: only Linux can harden a sandbox.
#[cfg(not(target_os = "linux"))]
pub fn harden() -> Result<()> {
    Err(crate::error::refused(MECHANISM_LANDLOCK))
}

/// Whether this kernel enforces the Landlock ruleset [`harden`] installs.
#[cfg(target_os = "linux")]
#[must_use]
pub fn landlock_enforceable() -> bool {
    linux::ruleset_supported()
}

/// Only Linux has Landlock.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn landlock_enforceable() -> bool {
    false
}

/// `/proc/<pid>/status` text, parsed, or a refusal: a status that does not
/// parse proves nothing about the process.
fn parse(status: &str) -> Result<Status> {
    Status::from_read(status.as_bytes())
        .map_err(|_unparsed| unconfined("the process status does not parse"))
}

/// Refuses unless exactly one thread exists, read from `/proc/<pid>/status` text.
///
/// # Errors
/// More than one thread, or a status that does not parse.
pub fn single_threaded(status: &str) -> Result<()> {
    if parse(status)?.threads == 1 {
        Ok(())
    } else {
        Err(unconfined(
            "a second thread exists, so confinement would miss it",
        ))
    }
}

/// Refuses unless the effective and permitted capability sets are both empty,
/// read from `/proc/<pid>/status` text.
///
/// # Errors
/// Either set is non-empty, or the status does not parse.
pub fn capabilities_dropped(status: &str) -> Result<()> {
    let status = parse(status)?;
    if status.capeff == 0 && status.capprm == 0 {
        Ok(())
    } else {
        Err(unconfined(CAPABILITY_SURVIVED))
    }
}

#[cfg(target_os = "linux")]
mod confinement;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use self::confinement::{Applying, Confinement, Hardened, Refused};
#[cfg(target_os = "linux")]
pub use self::linux::{LANDLOCK_ABI, REFUSED as REFUSED_SYSCALLS, X32_SYSCALL_BIT};

#[cfg(test)]
mod tests;
