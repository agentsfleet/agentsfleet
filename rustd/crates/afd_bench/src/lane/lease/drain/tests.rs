use super::{IDLE_COMMITS_PER_POLL, IDLE_STATEMENTS_PER_POLL, READY_DEPTH, summary};
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

/// An empty lease report, as the lane starts one.
fn report() -> Report {
    Report::new(Lane::Lease, Profile::Rig, Provenance::for_test())
}

#[test]
fn the_summary_prints_each_measured_key_as_the_rubric_greps_it() {
    let mut report = report();
    report.measurement(IDLE_STATEMENTS_PER_POLL, 14.5);
    report.measurement(IDLE_COMMITS_PER_POLL, 3.0);
    report.measurement(READY_DEPTH, 200.0);

    assert_eq!(
        summary(&report),
        "idle_statements_per_poll=14.5 idle_commits_per_poll=3 drain_ready_depth=200"
    );
}

#[test]
fn a_zero_prints_as_a_bare_zero() {
    // The rubric's pass condition is this exact token, so a zero must not
    // render as `0.0`, which it would still match, or be dropped, which it
    // would not.
    let mut report = report();
    report.measurement(IDLE_STATEMENTS_PER_POLL, 0.0);

    assert_eq!(summary(&report), "idle_statements_per_poll=0");
}

#[test]
fn a_key_the_run_never_measured_is_left_out_rather_than_printed_as_zero() {
    // An aborted drain writes no idle window. Printing its absent cost as zero
    // would pass the rubric on a run that never polled.
    assert_eq!(summary(&report()), "");
}
