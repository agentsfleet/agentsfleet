use core::time::Duration;

use super::turn::Turn;
use super::{IDLE_COMMITS, IDLE_STATEMENTS, UNSETTLED_ROWS, summary};
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

/// An empty lease report, as the lane starts one.
fn report() -> Report {
    Report::new(Lane::Lease, Profile::Rig, Provenance::for_test())
}

#[test]
fn the_summary_prints_each_measured_key_as_the_rubric_greps_it() {
    let mut report = report();
    report.measurement(IDLE_STATEMENTS, 14.5);
    report.measurement(IDLE_COMMITS, 3.0);
    report.measurement(UNSETTLED_ROWS, 200.0);

    assert_eq!(
        summary(&report),
        "drain_idle_statements=14.5 drain_idle_commits=3 drain_unsettled_rows=200"
    );
}

#[test]
fn a_zero_prints_as_a_bare_zero() {
    // The rubric's pass condition is this exact token, so a zero must not
    // render as `0.0`, which it would still match, or be dropped, which it
    // would not.
    let mut report = report();
    report.measurement(IDLE_STATEMENTS, 0.0);

    assert_eq!(summary(&report), "drain_idle_statements=0");
}

#[test]
fn a_key_the_run_never_measured_is_left_out_rather_than_printed_as_zero() {
    // An aborted drain writes no idle window. Printing its absent cost as zero
    // would pass the rubric on a run with no complete idle window.
    assert_eq!(summary(&report()), "");
}

#[test]
fn short_turn_sends_no_renewal() {
    let turn = Turn::back_to_back(Duration::from_millis(1_000));

    assert_eq!(
        (turn.renewals, turn.posts, turn.hold),
        (0, 4, None),
        "a second-long turn ends before the first renewal tick"
    );
}

#[test]
fn default_turn_renews_twice_and_posts_forty_times() {
    let turn = Turn::back_to_back(Duration::from_secs(10));

    assert_eq!((turn.renewals, turn.posts), (2, 40));
}

#[test]
fn a_held_turn_keeps_its_sends_and_waits_the_turn() {
    let turn = Turn::held(Duration::from_secs(2));

    assert_eq!(
        (turn.renewals, turn.posts, turn.hold),
        (0, 8, Some(Duration::from_secs(2)))
    );
}
