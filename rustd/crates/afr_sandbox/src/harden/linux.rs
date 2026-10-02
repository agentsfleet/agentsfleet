//! The two kernel calls that confine: a Landlock ruleset and a seccomp program.

use std::collections::BTreeMap;

use landlock::{
    ABI, Access as _, AccessFs, CompatLevel, Compatible as _, Ruleset, RulesetAttr as _,
    RulesetCreatedAttr as _, path_beneath_rules,
};
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule, TargetArch, sock_filter};

use super::{WRITABLE, WRITABLE_DEVICES};
use crate::error::Result;

/// The oldest Landlock interface the ruleset needs: Linux 6.2, which governs
/// renames across directories and truncation as well as writes.
const LANDLOCK_ABI: ABI = ABI::V3;
/// The root, readable and executable everywhere.
const ROOT: &str = "/";

/// System calls a sandboxed process gets `EPERM` for. Each opens a door the
/// namespaces leave: a second kernel interface (`io_uring`), another process's
/// memory, a namespace of its own, kernel programs, the kernel keyring, or
/// performance counters.
pub(crate) const REFUSED: [libc::c_long; 12] = [
    libc::SYS_io_uring_setup,
    libc::SYS_io_uring_enter,
    libc::SYS_io_uring_register,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_unshare,
    libc::SYS_bpf,
    libc::SYS_keyctl,
    libc::SYS_add_key,
    libc::SYS_request_key,
    libc::SYS_perf_event_open,
];

/// The bit an x86-64 kernel reads as "this is an x32 call". The kernel's
/// filter sees the number with the bit set and the same architecture, so a
/// filter keyed on numbers alone — `seccompiler`'s, and Codex's, which builds
/// on it — is walked around by `keyctl | X32_SYSCALL_BIT` wherever x32 is
/// enabled. No architecture numbers a real call this high, so every call that
/// carries the bit is refused, on every architecture.
pub(crate) const X32_SYSCALL_BIT: u32 = 0x4000_0000;

/// Reads everywhere; writes only beneath [`WRITABLE`] and to [`WRITABLE_DEVICES`].
pub(super) fn restrict_file_system() -> Result<()> {
    // A hard requirement: a kernel that can enforce only part of the ruleset
    // is an error here, never a partially confined sandbox.
    let (read, write) = (
        AccessFs::from_read(LANDLOCK_ABI),
        AccessFs::from_all(LANDLOCK_ABI),
    );
    let rules = path_beneath_rules([ROOT], read)
        .chain(path_beneath_rules(WRITABLE, write))
        .chain(path_beneath_rules(WRITABLE_DEVICES, write));
    Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(write)?
        .create()?
        .add_rules(rules)?
        .restrict_self()?;
    Ok(())
}

/// Installs the programs that answer [`REFUSED`], and every call numbered at
/// or past [`X32_SYSCALL_BIT`], with `EPERM`. Two programs, stacked: the
/// kernel runs both and the refusal wins.
pub(super) fn refuse_system_calls() -> Result<()> {
    seccompiler::apply_filter(&program()?)?;
    seccompiler::apply_filter(&high_numbers())?;
    Ok(())
}

/// The program refusing every call numbered at or past [`X32_SYSCALL_BIT`].
///
/// Four classic BPF instructions, because `seccompiler` can match a call only
/// by its exact number and has no way to say "this number or higher".
pub(crate) fn high_numbers() -> [sock_filter; 4] {
    // `seccomp_data.nr`, the first field of what the program reads.
    const NUMBER_OFFSET: u32 = 0;
    let step = |code: u32, jt: u8, jf: u8, k: u32| sock_filter {
        code: u16::try_from(code).unwrap_or_default(),
        jt,
        jf,
        k,
    };
    let refuse = libc::SECCOMP_RET_ERRNO | libc::EPERM.unsigned_abs();
    [
        step(
            libc::BPF_LD | libc::BPF_W | libc::BPF_ABS,
            0,
            0,
            NUMBER_OFFSET,
        ),
        // At or past the bit: fall through to the refusal; below it: skip it.
        step(
            libc::BPF_JMP | libc::BPF_JGE | libc::BPF_K,
            0,
            1,
            X32_SYSCALL_BIT,
        ),
        step(libc::BPF_RET | libc::BPF_K, 0, 0, refuse),
        step(libc::BPF_RET | libc::BPF_K, 0, 0, libc::SECCOMP_RET_ALLOW),
    ]
}

/// Compiles the seccomp program for this machine's architecture.
pub(crate) fn program() -> Result<BpfProgram> {
    let rules: BTreeMap<i64, Vec<SeccompRule>> =
        REFUSED.into_iter().map(|call| (call, Vec::new())).collect();
    let (allow, refuse) = (
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM.unsigned_abs()),
    );
    let arch = TargetArch::try_from(std::env::consts::ARCH)?;
    let filter = SeccompFilter::new(rules, allow, refuse, arch)?;
    Ok(filter.try_into()?)
}
