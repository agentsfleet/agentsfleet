//! What the process inside a sandbox does to itself before it reads anything.
//!
//! In order: `no_new_privs`, then Landlock, then seccomp, then a check that no
//! capability survived. Each step binds only the calling thread and its future
//! children, so [`harden`] refuses to run once a second thread exists — the
//! executor's runtime starts after it, never before.

#[cfg(target_os = "linux")]
use std::fs;

use crate::error::{Result, unconfined};

/// The process status file the checks read.
#[cfg(target_os = "linux")]
const PROC_SELF_STATUS: &str = "/proc/self/status";
/// The status line counting this process's threads.
const THREADS: &str = "Threads:";
/// The status line holding the effective capability set, in hexadecimal.
const CAP_EFFECTIVE: &str = "CapEff:";
/// The status line holding the permitted capability set, in hexadecimal.
const CAP_PERMITTED: &str = "CapPrm:";

/// Trees a sandboxed process may write beneath; everything else is read-only.
pub const WRITABLE: [&str; 4] = ["/workspace", "/tmp", "/run/agentsfleet", "/dev/pts"];
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

/// One field of a `/proc/<pid>/status` text, trimmed.
fn field<'a>(status: &'a str, key: &str) -> Option<&'a str> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .map(str::trim)
}

/// Refuses unless exactly one thread exists, read from `/proc/<pid>/status` text.
///
/// # Errors
/// More than one thread, or no thread count at all.
pub fn single_threaded(status: &str) -> Result<()> {
    match field(status, THREADS).and_then(|count| count.parse::<u32>().ok()) {
        Some(1) => Ok(()),
        Some(_) => Err(unconfined(
            "a second thread exists, so confinement would miss it",
        )),
        None => Err(unconfined("the process status does not count threads")),
    }
}

/// Refuses unless the effective and permitted capability sets are both empty,
/// read from `/proc/<pid>/status` text.
///
/// # Errors
/// Either set is non-empty or missing.
pub fn capabilities_dropped(status: &str) -> Result<()> {
    let empty = |key| {
        field(status, key)
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
            .is_some_and(|set| set == 0)
    };
    if empty(CAP_EFFECTIVE) && empty(CAP_PERMITTED) {
        Ok(())
    } else {
        Err(unconfined("a capability survived"))
    }
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(test)]
mod tests;
