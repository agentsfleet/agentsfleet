#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on the refusal it expects"
)]

use super::{capabilities_dropped, single_threaded};

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
fn test_the_seccomp_program_compiles_for_this_machine() {
    let program = super::linux::program();

    assert!(program.is_ok_and(|instructions| instructions.len() > 10));
}

#[cfg(target_os = "linux")]
#[test]
fn test_the_high_number_program_refuses_at_the_x32_bit_and_allows_below() {
    let [load, compare, refuse, allow] = super::linux::high_numbers();

    assert_eq!((load.code, load.k), (0x20, 0), "loads the call number");
    assert_eq!((compare.jt, compare.jf, compare.k), (0, 1, 0x4000_0000));
    assert_eq!(refuse.k, 0x0005_0000 | 1, "EPERM");
    assert_eq!(allow.k, 0x7fff_0000);
}
