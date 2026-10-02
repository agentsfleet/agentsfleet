#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on the refusal it expects"
)]

use super::{capabilities_dropped, single_threaded};

const CONFINED: &str = "Name:\tagentsfleet-run\nThreads:\t1\nCapInh:\t0000000000000000\n\
                        CapPrm:\t0000000000000000\nCapEff:\t0000000000000000\n";

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
    // No thread count is refused too.
    single_threaded("Name:\tx\n").unwrap_err();
}

#[test]
fn test_a_surviving_capability_is_refused() {
    let effective = CONFINED.replace("CapEff:\t0000000000000000", "CapEff:\t0000000000200000");
    let permitted = CONFINED.replace("CapPrm:\t0000000000000000", "CapPrm:\t000001ffffffffff");

    capabilities_dropped(&effective).unwrap_err();
    capabilities_dropped(&permitted).unwrap_err();
    // Unreadable and unparseable sets are refused.
    capabilities_dropped("Threads:\t1\n").unwrap_err();
    capabilities_dropped("CapPrm:\tzz\nCapEff:\t0\n").unwrap_err();
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
