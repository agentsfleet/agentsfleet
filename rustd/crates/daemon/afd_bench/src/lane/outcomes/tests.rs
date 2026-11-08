//! What folds, what counts as a poll, and what a failure is not.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;

use super::Outcomes;

#[test]
fn test_failures_are_neither_attempts_nor_wasted_claims() {
    let mut outcomes = Outcomes::new().expect("buildable");
    for _ in 0..5 {
        outcomes.failed();
    }

    assert_eq!(outcomes.attempts(), 0, "a refused poll answered nothing");
    assert!((outcomes.wasted_fraction() - 0.0).abs() < f64::EPSILON);
    assert!((outcomes.failure_fraction() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_a_fold_adds_every_count_and_every_sample() {
    let mut whole = Outcomes::new().expect("buildable");
    whole
        .succeeded(Duration::from_millis(1))
        .expect("recordable");
    let mut part = Outcomes::new().expect("buildable");
    part.missed(Duration::from_millis(3)).expect("recordable");
    part.missed(Duration::from_millis(3)).expect("recordable");
    part.failed();

    whole
        .absorb(&part)
        .expect("two histograms of one precision merge");

    assert_eq!((whole.successes, whole.misses, whole.failures), (1, 2, 1));
    assert_eq!(whole.attempts(), 3);
    assert_eq!(
        whole.latency.count(),
        3,
        "every timed sample survives the fold"
    );
    assert!((whole.wasted_fraction() - 2.0 / 3.0).abs() < 1e-9);
}

#[test]
fn test_an_empty_window_has_no_fractions_to_report() {
    let outcomes = Outcomes::new().expect("buildable");
    assert!((outcomes.wasted_fraction() - 0.0).abs() < f64::EPSILON);
    assert!((outcomes.failure_fraction() - 0.0).abs() < f64::EPSILON);
    assert!(outcomes.latency.is_empty());
}
