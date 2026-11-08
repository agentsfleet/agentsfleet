//! When a run stops, and when one error is not a reason to.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{Abort, MINIMUM_SAMPLE};

/// A threshold a deployed profile might carry.
const THRESHOLD: f64 = 0.10;

#[test]
fn test_a_run_aborts_when_the_target_starts_failing() {
    let abort = Abort::new(THRESHOLD);

    // Healthy for the minimum sample, then the target starts refusing.
    for _ in 0..MINIMUM_SAMPLE {
        abort.record(true);
    }
    assert!(!abort.fired(), "a healthy sample must not stop the run");
    for _ in 0..MINIMUM_SAMPLE {
        abort.record(false);
    }

    assert!(
        abort.fired(),
        "half the operations failing is past a ten percent threshold"
    );
    let recorded = abort.recorded().expect("a fired abort is recorded");
    assert!(
        (recorded.threshold - THRESHOLD).abs() < f64::EPSILON,
        "the result names the threshold that stopped it"
    );
    assert!(
        recorded.observed_error_rate > THRESHOLD,
        "the result records the rate that crossed it, said {}",
        recorded.observed_error_rate
    );
}

#[test]
fn test_one_early_error_does_not_abort_a_run() {
    let abort = Abort::new(THRESHOLD);

    abort.record(false);

    assert!(
        !abort.fired(),
        "the first call failing is a 100% rate over a sample of one, and \
         judging it would abort every run that hit a cold connection"
    );
    assert!(
        abort.recorded().is_none(),
        "nothing fired, nothing recorded"
    );
}

#[test]
fn test_a_rate_at_the_threshold_is_allowed_and_one_past_it_is_not() {
    let at = Abort::new(0.5);
    for _ in 0..MINIMUM_SAMPLE / 2 {
        at.record(true);
        at.record(false);
    }
    assert!(
        !at.fired(),
        "exactly the threshold is the ceiling, not a refusal"
    );

    let past = Abort::new(0.5);
    for _ in 0..MINIMUM_SAMPLE / 2 {
        past.record(true);
        past.record(false);
    }
    past.record(false);
    assert!(past.fired(), "one past the ceiling stops the run");
}

#[test]
fn test_the_token_is_what_a_driver_loop_watches() {
    let abort = Abort::new(0.0);
    let token = abort.token();
    for _ in 0..MINIMUM_SAMPLE {
        abort.record(false);
    }
    assert!(
        token.is_cancelled(),
        "a handed-out token sees the cancellation"
    );
}
