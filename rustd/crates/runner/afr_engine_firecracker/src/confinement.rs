//! What this host's kernel offers a confined process: Landlock to confine its
//! file system, and seccomp to filter its system calls. The capability report
//! carries both facts.
//!
//! Each listing is read from a path the caller can replace, so a test states a
//! host rather than needing one; whether the kernel builds the Landlock
//! ruleset the guest's confinement installs is [`landlock_enforceable`]. The
//! guest's own crate is never linked here, so the ruleset's interface version
//! is named in both places and `af_e2e` holds the two equal.

use std::fs;
use std::path::Path;

use afr_engine::MECHANISM_LANDLOCK;

/// Where the kernel lists its active security modules.
pub const LSM_PATH: &str = "/sys/kernel/security/lsm";
/// Where the kernel lists the seccomp actions it supports.
pub const SECCOMP_ACTIONS_PATH: &str = "/proc/sys/kernel/seccomp/actions_avail";
/// The seccomp action every refused system call answers with.
const SECCOMP_ERRNO_ACTION: &str = "errno";

/// The oldest Landlock interface the guest's ruleset needs; equal to
/// `afg_confine::LANDLOCK_ABI`.
#[cfg(target_os = "linux")]
pub const LANDLOCK_ABI: landlock::ABI = landlock::ABI::V3;

/// Whether this kernel enforces the Landlock ruleset the guest's confinement
/// installs: it asks the kernel and changes nothing.
#[cfg(target_os = "linux")]
#[must_use]
pub fn landlock_enforceable() -> bool {
    use landlock::{
        Access as _, AccessFs, CompatLevel, Compatible as _, Ruleset, RulesetAttr as _,
    };

    Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(LANDLOCK_ABI))
        .is_ok()
}

/// Only Linux has Landlock.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn landlock_enforceable() -> bool {
    false
}

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
