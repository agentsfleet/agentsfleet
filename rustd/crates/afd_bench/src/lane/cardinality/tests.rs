//! The ladder's rungs, and what a plan line yields.

use core::time::Duration;

use super::probe::{EXECUTION_TIME, PLANNING_TIME, median_ms, plan_time};
use super::rungs;

#[test]
fn test_the_ladder_climbs_by_tens_to_its_ceiling() {
    // pin test: literal is the contract
    assert_eq!(rungs(10_000), vec![10, 100, 1_000, 10_000]);
    // pin test: literal is the contract
    assert_eq!(rungs(1_000_000), vec![1_000, 10_000, 100_000, 1_000_000]);
}

#[test]
fn test_a_small_ceiling_drops_the_rungs_that_would_be_empty() {
    assert_eq!(rungs(5), vec![5], "a rung of zero fleets measures nothing");
    assert_eq!(rungs(200), vec![2, 20, 200]);
}

#[test]
fn test_a_ceiling_that_is_itself_a_rung_is_not_reported_twice() {
    // pin test: literal is the contract
    assert_eq!(rungs(1_000), vec![1, 10, 100, 1_000]);
}

#[test]
fn test_plan_times_are_read_off_their_labelled_lines() {
    let plan: Vec<String> = [
        "Limit  (cost=1.00..2.00 rows=64 width=32) (actual time=0.10..0.40 rows=8 loops=1)",
        "Planning Time: 1.537 ms",
        "Execution Time: 0.531 ms",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    assert_eq!(plan_time(&plan, EXECUTION_TIME), Some(0.531));
    assert_eq!(plan_time(&plan, PLANNING_TIME), Some(1.537));
}

#[test]
fn test_a_plan_without_timing_reports_no_time_rather_than_zero() {
    let untimed: Vec<String> = vec!["Seq Scan on fleets".to_owned()];
    assert_eq!(plan_time(&untimed, EXECUTION_TIME), None);
}

#[test]
fn test_a_ceiling_of_zero_has_no_rungs() {
    assert!(
        rungs(0).is_empty(),
        "a rung of zero fleets measures nothing, including the top one"
    );
}

#[test]
fn test_the_median_is_the_middle_sample_and_nothing_has_none() {
    let odd = vec![
        Duration::from_millis(3),
        Duration::from_millis(1),
        Duration::from_millis(2),
    ];
    assert_eq!(median_ms(&odd), Some(2.0));
    let even = vec![Duration::from_millis(1), Duration::from_millis(4)];
    assert_eq!(
        median_ms(&even),
        Some(4.0),
        "the upper middle, so a tail is never rounded away"
    );
    assert_eq!(median_ms(&[]), None, "no samples is not a latency of zero");
}
