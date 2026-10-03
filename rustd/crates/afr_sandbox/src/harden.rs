//! What the process inside a sandbox does to itself before it reads anything.
//!
//! In order: `no_new_privs`, then Landlock, then seccomp, then a check that no
//! capability survived. Each step binds only the calling thread and its future
//! children, so [`harden`] refuses to run once a second thread exists — the
//! executor's runtime starts after it, never before.

#[cfg(target_os = "linux")]
use std::fs;

use procfs_core::FromRead as _;
use procfs_core::process::Status;

use crate::bubblewrap::{DEV_SHM, SANDBOX_TMP, SANDBOX_WORKSPACE};
use crate::error::{Result, unconfined};

/// The process status file the checks read.
#[cfg(target_os = "linux")]
const PROC_SELF_STATUS: &str = "/proc/self/status";
/// Pseudo-terminals, which `--dev` mounts and an interactive process writes.
const DEV_PTS: &str = "/dev/pts";

/// Trees a sandboxed process may write beneath; everything else is read-only.
///
/// The executor's socket directory is not among them: the socket is bound
/// before confinement, and the directory is a host path no tenant may fill.
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
    rustix::thread::set_no_new_privs(true)?;
    linux::restrict_file_system()?;
    linux::refuse_system_calls()?;
    capabilities_dropped(&fs::read_to_string(PROC_SELF_STATUS)?)
}

/// A host without Landlock cannot confine a sandbox, so it never serves one.
///
/// # Errors
/// Always: only Linux can harden a sandbox.
#[cfg(not(target_os = "linux"))]
pub fn harden() -> Result<()> {
    Err(crate::error::refused(crate::probe::MECHANISM_LANDLOCK))
}

/// Whether this kernel enforces the Landlock ruleset [`harden`] installs.
#[cfg(target_os = "linux")]
pub(crate) fn landlock_enforceable() -> bool {
    linux::ruleset_supported()
}

/// Only Linux has Landlock.
#[cfg(not(target_os = "linux"))]
pub(crate) const fn landlock_enforceable() -> bool {
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
        Err(unconfined("a capability survived"))
    }
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(test)]
mod tests;
