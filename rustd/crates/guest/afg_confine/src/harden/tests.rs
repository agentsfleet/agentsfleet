#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on the refusal it expects"
)]

use std::path::Path;

use super::{WRITABLE, WRITABLE_DEVICES, capabilities_dropped, single_threaded};

/// The guest's console is the guest kernel's and the agent's alone: the
/// runner logs every line of it as the sandbox's own (`sandbox_console`), so
/// no tenant process may write a line there. Landlock grants a tenant writes
/// beneath [`WRITABLE`] and to [`WRITABLE_DEVICES`] and nothing else, and
/// neither names the console, the serial port it is, or the kernel's log.
#[test]
fn test_a_tenant_can_write_neither_the_console_nor_the_kernel_log() {
    for console in ["/dev/console", "/dev/ttyS0", "/dev/kmsg"] {
        assert!(
            !WRITABLE_DEVICES.contains(&console),
            "{console} is writable"
        );
        assert!(
            !WRITABLE
                .iter()
                .any(|tree| Path::new(console).starts_with(tree)),
            "{console} lies under a writable tree"
        );
    }
}

/// A whole `/proc/<pid>/status`, as a confined sandbox process reads its own:
/// the parser takes nothing less, and a fragment proves nothing about one.
const CONFINED: &str = "Name:\tagentsfleet-run\nUmask:\t0022\nState:\tR (running)\n\
Tgid:\t7\nNgid:\t0\nPid:\t7\nPPid:\t1\nTracerPid:\t0\nUid:\t1000\t1000\t1000\t1000\n\
Gid:\t1000\t1000\t1000\t1000\nFDSize:\t64\nGroups:\t1000 \nNStgid:\t7\nNSpid:\t7\n\
NSpgid:\t1\nNSsid:\t1\nVmPeak:\t    8012 kB\nVmSize:\t    8012 kB\nVmLck:\t       0 kB\n\
VmPin:\t       0 kB\nVmHWM:\t    1652 kB\nVmRSS:\t    1652 kB\nRssAnon:\t     100 kB\n\
RssFile:\t    1552 kB\nRssShmem:\t       0 kB\nVmData:\t     344 kB\nVmStk:\t     132 kB\n\
VmExe:\t      28 kB\nVmLib:\t    1804 kB\nVmPTE:\t      52 kB\nVmSwap:\t       0 kB\n\
Threads:\t1\nSigQ:\t0/32138\nSigPnd:\t0000000000000000\nShdPnd:\t0000000000000000\n\
SigBlk:\t0000000000000000\nSigIgn:\t0000000000000000\nSigCgt:\t0000000000000000\n\
CapInh:\t0000000000000000\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\n\
CapBnd:\t0000000000000000\nCapAmb:\t0000000000000000\nNoNewPrivs:\t1\nSeccomp:\t2\n\
Seccomp_filters:\t2\nCpus_allowed:\t7f\nCpus_allowed_list:\t0-6\nMems_allowed:\t1\n\
Mems_allowed_list:\t0\nvoluntary_ctxt_switches:\t2\nnonvoluntary_ctxt_switches:\t0\n";

#[test]
fn test_a_single_threaded_process_with_no_capabilities_passes() {
    single_threaded(CONFINED).unwrap();
    capabilities_dropped(CONFINED).unwrap();
}

#[test]
fn test_a_second_thread_is_refused_because_confinement_would_miss_it() {
    let two = CONFINED.replace("Threads:\t1", "Threads:\t2");

    let refused = single_threaded(&two).err().map(|error| error.to_string());

    assert!(refused.is_some_and(|text| text.contains("second thread")));
    // A status that does not parse is refused too.
    single_threaded("Name:\tx\n").unwrap_err();
}

#[test]
fn test_a_surviving_capability_is_refused() {
    let effective = CONFINED.replace("CapEff:\t0000000000000000", "CapEff:\t0000000000200000");
    let permitted = CONFINED.replace("CapPrm:\t0000000000000000", "CapPrm:\t000001ffffffffff");

    capabilities_dropped(&effective).unwrap_err();
    capabilities_dropped(&permitted).unwrap_err();
    // A status that does not parse proves nothing, so it is refused.
    capabilities_dropped("Threads:\t1\n").unwrap_err();
}

#[cfg(not(target_os = "linux"))]
#[test]
fn test_harden_refuses_where_there_is_no_landlock() {
    let refused = super::harden().unwrap_err();

    assert_eq!(refused.missing_mechanism(), Some("landlock"));
}

#[cfg(target_os = "linux")]
#[test]
fn test_the_seccomp_programs_compile_for_this_machine() {
    let [refused, unsupported] = super::linux::programs().unwrap();

    assert!(refused.len() > 10, "{}", refused.len());
    assert!(unsupported.len() > 4, "{}", unsupported.len());
}

#[cfg(target_os = "linux")]
#[test]
fn test_the_high_number_program_refuses_at_the_x32_bit_and_allows_below() {
    let [load, compare, refuse, allow] = super::linux::HIGH_NUMBERS;

    assert_eq!((load.code, load.k), (0x20, 0), "loads the call number");
    assert_eq!((compare.jt, compare.jf, compare.k), (0, 1, 0x4000_0000));
    assert_eq!(refuse.k, 0x0005_0000 | 1, "EPERM");
    assert_eq!(allow.k, 0x7fff_0000);
}

#[cfg(target_os = "linux")]
#[test]
fn test_a_filter_instruction_preserves_its_opcode_jumps_and_operand() {
    let instruction = super::linux::step(
        std::hint::black_box(u32::from(u16::MAX)),
        7,
        11,
        0x4000_0000,
    );
    assert_eq!(instruction.code, u16::MAX);
    assert_eq!(instruction.jt, 7);
    assert_eq!(instruction.jf, 11);
    assert_eq!(instruction.k, 0x4000_0000);
}

#[cfg(target_os = "linux")]
#[test]
#[should_panic(expected = "a BPF opcode is sixteen bits")]
fn test_a_filter_instruction_refuses_an_opcode_wider_than_sixteen_bits() {
    super::linux::step(std::hint::black_box(u32::from(u16::MAX) + 1), 0, 0, 0);
}

/// A refused step reaches a process between fork and exec as the kernel's
/// own answer, or a denied permission where no system call gave one, and
/// reaches every other caller as the failure the step always was.
#[cfg(target_os = "linux")]
#[test]
fn test_a_refused_step_keeps_its_answer_for_a_fork_and_its_sentence_for_a_caller() {
    use std::io;

    use super::Refused;
    use crate::Error;

    let steps = || {
        let missing = landlock::CreateRulesetError::MissingHandledAccess;
        [
            Refused::System(rustix::io::Errno::NOSYS),
            Refused::Landlock(landlock::RulesetError::CreateRuleset(missing)),
            Refused::Seccomp(seccompiler::Error::EmptyFilter),
            Refused::Capability,
            Refused::Spent,
        ]
    };

    let answers = steps().map(|step| {
        let answer = io::Error::from(step);
        (answer.raw_os_error(), answer.kind())
    });
    let sentences = steps().map(|step| Error::from(step).detail());

    let denied = (None, io::ErrorKind::PermissionDenied);
    assert_eq!(answers[0].0, Some(libc::ENOSYS));
    assert_eq!(answers[1..], [denied; 4]);
    let told = [
        "a system call was refused",
        "Landlock refused the file-system ruleset",
        "the kernel refused the seccomp program",
        "a capability survived",
        "the confinement was applied already",
    ];
    for (sentence, told) in sentences.iter().zip(told) {
        assert!(sentence.contains(told), "{sentence}");
    }
}
