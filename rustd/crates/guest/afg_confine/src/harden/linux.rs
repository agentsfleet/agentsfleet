//! The two kernel calls that confine: a Landlock ruleset and a seccomp program.

use std::collections::BTreeMap;

use landlock::{
    ABI, Access as _, AccessFs, CompatLevel, Compatible as _, Ruleset, RulesetAttr as _,
    RulesetCreated, RulesetCreatedAttr as _, path_beneath_rules,
};
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, TargetArch, sock_filter,
};

use super::{WRITABLE, WRITABLE_DEVICES};
use crate::error::Result;

/// The oldest Landlock interface the ruleset needs: Linux 6.2, which governs
/// renames across directories and truncation as well as writes.
pub const LANDLOCK_ABI: ABI = ABI::V3;
/// The root, readable and executable everywhere.
const ROOT: &str = "/";

/// System calls a sandboxed process gets `EPERM` for.
///
/// Each opens a door the namespaces leave: a second kernel interface
/// (`io_uring`), another process's memory, a namespace of its own or
/// another's, kernel programs, the kernel keyring, or performance counters.
/// `clone` and `clone3` make a namespace too, and are held by
/// [`NEW_NAMESPACES`] and [`UNSUPPORTED`]: neither can be refused whole.
pub const REFUSED: [libc::c_long; 13] = [
    libc::SYS_io_uring_setup,
    libc::SYS_io_uring_enter,
    libc::SYS_io_uring_register,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_unshare,
    libc::SYS_setns,
    libc::SYS_bpf,
    libc::SYS_keyctl,
    libc::SYS_add_key,
    libc::SYS_request_key,
    libc::SYS_perf_event_open,
];

/// The flags `clone` makes a namespace with; a `clone` carrying any of them
/// gets `EPERM`, as `unshare` does, and every other `clone`, a thread or a
/// fork, runs. A time namespace has no `clone` flag: its bit is one of the
/// exit signal's there.
const NEW_NAMESPACES: [libc::c_int; 7] = [
    libc::CLONE_NEWNS,
    libc::CLONE_NEWCGROUP,
    libc::CLONE_NEWUTS,
    libc::CLONE_NEWIPC,
    libc::CLONE_NEWUSER,
    libc::CLONE_NEWPID,
    libc::CLONE_NEWNET,
];
/// Which of `clone`'s arguments holds its flags, on every architecture
/// `seccompiler` compiles for.
const CLONE_FLAGS_ARGUMENT: u8 = 0;

/// System calls a sandboxed process gets `ENOSYS` for, as from a kernel that
/// never had them.
///
/// `clone3` takes its flags behind a pointer, which no seccomp program can
/// read, so its namespace flags cannot be told from a thread's. glibc tries
/// it first and falls back to `clone` on exactly this answer, musl calls
/// `clone` alone, and `EPERM` here would fail every thread a glibc program
/// starts.
const UNSUPPORTED: [libc::c_long; 1] = [libc::SYS_clone3];

/// The bit an x86-64 kernel reads as "this is an x32 call".
///
/// The kernel's filter sees the number with the bit set and the same
/// architecture, so a filter keyed on numbers alone — `seccompiler`'s — is
/// walked around by `keyctl | X32_SYSCALL_BIT` wherever x32 is enabled. No
/// architecture numbers a real call this high, so every call that carries the
/// bit is refused, on every architecture.
pub const X32_SYSCALL_BIT: u32 = 0x4000_0000;

/// The ruleset a tenant process enforces on itself, built and not yet
/// enforced: reads everywhere; writes only beneath [`WRITABLE`] and to
/// [`WRITABLE_DEVICES`]. A process that confines itself between fork and
/// exec, where nothing may be allocated, enforces one built for it before
/// the fork.
pub(super) fn file_system_rules() -> Result<RulesetCreated> {
    let read = AccessFs::from_read(LANDLOCK_ABI);
    let write = AccessFs::from_all(LANDLOCK_ABI);
    let rules = path_beneath_rules([ROOT], read)
        .chain(path_beneath_rules(WRITABLE, write))
        .chain(path_beneath_rules(WRITABLE_DEVICES, write));
    Ok(ruleset()?.create()?.add_rules(rules)?)
}

/// Whether this kernel builds the ruleset every sandbox is confined by. It
/// asks the kernel and changes nothing; listing Landlock as a security module
/// is not enough, since a kernel older than [`LANDLOCK_ABI`] lists it and then
/// refuses the ruleset.
pub(super) fn ruleset_supported() -> bool {
    ruleset().is_ok()
}

/// The ruleset every sandbox is confined by, before its rules are added. A
/// hard requirement: a kernel that can enforce only part of it is refused
/// here, never left as a partially confined sandbox.
fn ruleset() -> Result<Ruleset> {
    Ok(Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(LANDLOCK_ABI))?)
}

/// The program refusing every call numbered at or past [`X32_SYSCALL_BIT`],
/// installed after [`programs`]. Stacked: the kernel runs them all and a
/// refusal wins.
///
/// Four classic BPF instructions, because `seccompiler` can match a call only
/// by its exact number and has no way to say "this number or higher". Built
/// at compile time, so an opcode that did not fit is a build failure.
pub(super) const HIGH_NUMBERS: [sock_filter; 4] = {
    // `seccomp_data.nr`, the first field of what the program reads.
    const NUMBER_OFFSET: u32 = 0;
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
};

/// One instruction. The kernel reads an opcode as sixteen bits; one wider is
/// refused while the program is built rather than cut to another opcode.
pub(super) const fn step(code: u32, jt: u8, jf: u8, k: u32) -> sock_filter {
    assert!(code <= u16::MAX as u32, "a BPF opcode is sixteen bits");
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the assertion above holds the opcode to sixteen bits"
    )]
    sock_filter {
        code: code as u16,
        jt,
        jf,
        k,
    }
}

/// Compiles the seccomp programs for this machine's architecture: one
/// answering [`REFUSED`], and a `clone` that makes a namespace, with `EPERM`;
/// one answering [`UNSUPPORTED`] with `ENOSYS`. Two, because a `seccompiler`
/// filter gives every call it matches one answer.
pub(super) fn programs() -> Result<[BpfProgram; 2]> {
    let namespaces = NEW_NAMESPACES
        .into_iter()
        .map(|flag| {
            let flag = u64::from(flag.unsigned_abs());
            // The low half alone, as the kernel reads `clone`'s flags.
            let carried = SeccompCondition::new(
                CLONE_FLAGS_ARGUMENT,
                SeccompCmpArgLen::Dword,
                SeccompCmpOp::MaskedEq(flag),
                flag,
            )?;
            SeccompRule::new(vec![carried])
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let refused: BTreeMap<i64, Vec<SeccompRule>> = REFUSED
        .into_iter()
        .map(|call| (call, Vec::new()))
        .chain([(libc::SYS_clone, namespaces)])
        .collect();
    let unsupported: BTreeMap<i64, Vec<SeccompRule>> = UNSUPPORTED
        .into_iter()
        .map(|call| (call, Vec::new()))
        .collect();
    let arch = TargetArch::try_from(std::env::consts::ARCH)?;
    let answering = |rules, errno: libc::c_int| {
        let answer = SeccompAction::Errno(errno.unsigned_abs());
        SeccompFilter::new(rules, SeccompAction::Allow, answer, arch)
    };
    Ok([
        answering(refused, libc::EPERM)?.try_into()?,
        answering(unsupported, libc::ENOSYS)?.try_into()?,
    ])
}
