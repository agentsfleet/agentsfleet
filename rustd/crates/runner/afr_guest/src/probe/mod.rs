//! What this host's kernel offers the process inside a sandbox: Landlock to
//! confine its file system, and seccomp to filter its system calls.
//!
//! Each fact is read from a path the caller can replace, so a test states a
//! host rather than needing one; whether the kernel builds the ruleset
//! [`crate::harden`] installs is [`crate::landlock_enforceable`].

use std::fs;
use std::path::Path;

use afr_engine::MECHANISM_LANDLOCK;

/// Where the kernel lists its active security modules.
pub const LSM_PATH: &str = "/sys/kernel/security/lsm";
/// Where the kernel lists the seccomp actions it supports.
pub const SECCOMP_ACTIONS_PATH: &str = "/proc/sys/kernel/seccomp/actions_avail";
/// The seccomp action every refused system call answers with.
const SECCOMP_ERRNO_ACTION: &str = "errno";

/// Whether the kernel's security-module list at `lsm` names Landlock. A list
/// that cannot be read names none.
#[must_use]
pub fn landlock_listed(lsm: &Path) -> bool {
    fs::read_to_string(lsm)
        .unwrap_or_default()
        .trim()
        .split(',')
        .any(|module| module == MECHANISM_LANDLOCK)
}

/// Whether the kernel's seccomp action list at `actions` offers the action
/// every refused system call answers with. A list that cannot be read offers
/// none.
#[must_use]
pub fn seccomp_offered(actions: &Path) -> bool {
    fs::read_to_string(actions)
        .unwrap_or_default()
        .split_whitespace()
        .any(|action| action == SECCOMP_ERRNO_ACTION)
}
