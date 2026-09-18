//! The blame label as it reaches the wire, not as `fault_of` decides it.
//!
//! `runner::tests` proves the CLASSIFICATION: eleven classes, each on the side
//! somebody chose, and `None` on ours. It cannot prove the label is attached —
//! or, for a success, that it is left off. Those are two different bugs, and
//! the second one is silent: a `fault` written onto `processed` would move
//! every successful run out of `{fault!="workload"}` and the objective would
//! read healthy while measuring nothing.
//!
//! Read back through the capturing exporter rather than inferred from the
//! classifier, because the classifier being right says nothing about which
//! attributes the counter was handed.
//!
//! # Why its own binary
//!
//! `producers::install` and `Capture::install` both write process-wide
//! `OnceLock`s, and `test_util` says a binary installing its own provider may
//! not share with a suite that installs another. `producers_drive.rs` installs
//! a plain provider and asserts no value; this one installs the capture and
//! asserts values, so the two cannot be the same binary.
#![cfg(feature = "test-util")]

use afd_observability::producers::fleet::runner;
use afd_observability::test_util::Capture;
use afd_wire::report::FailureClass;

/// The family the blame rides on.
const EXECUTIONS: &str = "agentsfleet_runner_executions_total";

const RUNNER_ID: &str = "runner_id";
const OUTCOME: &str = "outcome";
const FAULT: &str = "fault";

const FLEET_ERROR: &str = "fleet_error";
const PROCESSED: &str = "processed";
const PLATFORM: &str = "platform";
const WORKLOAD: &str = "workload";

/// One runner per test: the per-runner table is process-wide and never evicted,
/// so sharing an id would let one test read another's increment.
const OOM: &str = "runner-fault-oom";
const CRASH: &str = "runner-fault-crash";
const UNMODELLED: &str = "runner-fault-unmodelled";
const CLEAN: &str = "runner-fault-clean";

/// Every series this suite reads, as the exporter keys them.
///
/// `Capture::sum` matches the EXACT label set, which is what makes the absence
/// proof below possible: a success and a failure of the same runner are
/// different keys, and a success that grew a `fault` would stop matching the
/// two-label key entirely rather than quietly matching both.
fn failed_series(runner: &str, fault: &str) -> [(&'static str, String); 3] {
    [
        (FAULT, fault.to_owned()),
        (OUTCOME, FLEET_ERROR.to_owned()),
        (RUNNER_ID, runner.to_owned()),
    ]
}

fn sum(capture: &Capture, labels: &[(&str, String)]) -> u64 {
    let borrowed: Vec<(&str, &str)> = labels
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    capture.sum(EXECUTIONS, &borrowed)
}

#[test]
fn should_charge_a_workload_failure_to_the_workload() {
    let capture = Capture::install();
    runner::failed(OOM, Some(FailureClass::OomKill));

    assert_eq!(
        sum(&capture, &failed_series(OOM, WORKLOAD)),
        1,
        "a run that outgrew its own memory ceiling must not spend the platform's budget"
    );
    assert_eq!(
        sum(&capture, &failed_series(OOM, PLATFORM)),
        0,
        "the same failure must not also appear as ours"
    );
}

#[test]
fn should_charge_a_platform_failure_to_the_platform() {
    let capture = Capture::install();
    runner::failed(CRASH, Some(FailureClass::RunnerCrash));

    assert_eq!(
        sum(&capture, &failed_series(CRASH, PLATFORM)),
        1,
        "the runner process dying is ours and must reach the objective"
    );
    assert_eq!(
        sum(&capture, &failed_series(CRASH, WORKLOAD)),
        0,
        "our own crash must never be charged to the tenant"
    );
}

#[test]
fn should_charge_an_unclassified_failure_to_the_platform() {
    let capture = Capture::install();
    runner::failed(UNMODELLED, None);

    assert_eq!(
        sum(&capture, &failed_series(UNMODELLED, PLATFORM)),
        1,
        "a cause nobody could name is not evidence against the tenant"
    );
    assert_eq!(
        sum(&capture, &failed_series(UNMODELLED, WORKLOAD)),
        0,
        "an absent class must never read as the workload's fault"
    );
}

/// The one that makes `{fault!="workload"}` mean what the objective needs.
///
/// A not-equal matcher is satisfied by an ABSENT label, so a success carrying
/// no `fault` is selected by it and counts toward the denominator. Give the
/// success a `fault` of any value and it still matches — but it also becomes a
/// different series, and the two-label key below goes to zero. That is the
/// regression this pins.
#[test]
fn should_leave_a_success_without_a_fault_label() {
    let capture = Capture::install();
    runner::processed(CLEAN);

    let clean = [
        (OUTCOME, PROCESSED.to_owned()),
        (RUNNER_ID, CLEAN.to_owned()),
    ];
    assert_eq!(
        sum(&capture, &clean),
        1,
        "a success is keyed by outcome and runner alone; a third label here breaks the objective's denominator"
    );

    for fault in [PLATFORM, WORKLOAD] {
        let labelled = [
            (FAULT, fault.to_owned()),
            (OUTCOME, PROCESSED.to_owned()),
            (RUNNER_ID, CLEAN.to_owned()),
        ];
        assert_eq!(
            sum(&capture, &labelled),
            0,
            "a successful run must carry no fault at all, not fault={fault}"
        );
    }
}
