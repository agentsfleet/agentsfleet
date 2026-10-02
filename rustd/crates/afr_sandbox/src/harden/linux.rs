//! The two kernel calls that confine: a Landlock ruleset and a seccomp program.

use std::collections::BTreeMap;

use landlock::{
    ABI, Access as _, AccessFs, CompatLevel, Compatible as _, Ruleset, RulesetAttr as _,
    RulesetCreatedAttr as _, RulesetStatus, path_beneath_rules,
};
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule, TargetArch};

use super::{WRITABLE, WRITABLE_DEVICES};
use crate::error::{Result, unconfined};

/// The oldest Landlock interface the ruleset needs: Linux 6.2, which governs
/// renames across directories and truncation as well as writes.
const LANDLOCK_ABI: ABI = ABI::V3;
/// The root, readable and executable everywhere.
const ROOT: &str = "/";

/// System calls a sandboxed process gets `EPERM` for. Each opens a door the
/// namespaces leave: a second kernel interface (`io_uring`), another process's
/// memory, a namespace of its own, kernel programs, the kernel keyring, or
/// performance counters.
const REFUSED: [libc::c_long; 10] = [
    libc::SYS_io_uring_setup,
    libc::SYS_io_uring_enter,
    libc::SYS_io_uring_register,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_unshare,
    libc::SYS_bpf,
    libc::SYS_keyctl,
    libc::SYS_perf_event_open,
];

/// Reads everywhere; writes only beneath [`WRITABLE`] and to [`WRITABLE_DEVICES`].
pub(super) fn restrict_file_system() -> Result<()> {
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(LANDLOCK_ABI))?
        .create()?
        .add_rules(path_beneath_rules(
            [ROOT],
            AccessFs::from_read(LANDLOCK_ABI),
        ))?
        .add_rules(path_beneath_rules(
            WRITABLE,
            AccessFs::from_all(LANDLOCK_ABI),
        ))?
        .add_rules(path_beneath_rules(
            WRITABLE_DEVICES,
            AccessFs::from_all(LANDLOCK_ABI),
        ))?
        .restrict_self()?;
    if status.ruleset == RulesetStatus::FullyEnforced {
        Ok(())
    } else {
        Err(unconfined("Landlock is not fully enforced"))
    }
}

/// Installs the program that answers [`REFUSED`] with `EPERM`.
pub(super) fn refuse_system_calls() -> Result<()> {
    seccompiler::apply_filter(&program()?)?;
    Ok(())
}

/// Compiles the seccomp program for this machine's architecture.
pub(crate) fn program() -> Result<BpfProgram> {
    let rules: BTreeMap<i64, Vec<SeccompRule>> =
        REFUSED.into_iter().map(|call| (call, Vec::new())).collect();
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM.unsigned_abs()),
        TargetArch::try_from(std::env::consts::ARCH)?,
    )?;
    Ok(filter.try_into()?)
}
