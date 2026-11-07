//! What the distribution reports, and what it refuses to invent.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;

use super::{Latency, P95, P99};

/// How close a bucketed quantile must land to the value it stands for.
///
/// Three significant figures is a tenth of a percent; a whole percent gives
/// the assertion room for the bucket boundary without letting a wrong answer
/// through.
const TOLERANCE: f64 = 0.01;

/// Enough samples that a quantile is a quantile rather than one value.
const UNIFORM_SAMPLES: usize = 1_000;

#[test]
fn test_an_empty_distribution_reports_no_samples() {
    let latency = Latency::new().expect("the histogram must build");

    assert_eq!(latency.count(), 0);
    assert!(
        latency.is_empty(),
        "a lane that recorded nothing must be able to say so, rather than \
         reporting a zero somebody reads as a measurement"
    );
}

#[test]
fn test_a_recorded_sample_is_counted() {
    let mut latency = Latency::new().expect("the histogram must build");

    latency
        .record(Duration::from_millis(5))
        .expect("a five millisecond operation is recordable");

    assert_eq!(latency.count(), 1);
    assert!(!latency.is_empty());
}

#[test]
fn test_a_uniform_distribution_reports_that_value_at_every_quantile() {
    let mut latency = Latency::new().expect("the histogram must build");
    let each = Duration::from_millis(20);

    for _ in 0..UNIFORM_SAMPLES {
        latency.record(each).expect("recordable");
    }

    for quantile in [P95, P99] {
        let reported = latency.quantile_ms(quantile);
        assert!(
            (reported - 20.0).abs() < 20.0 * TOLERANCE,
            "a thousand identical samples must report that value at q{quantile}, said {reported}"
        );
    }
}

#[test]
fn test_the_tail_is_where_the_slow_operations_land() {
    let mut latency = Latency::new().expect("the histogram must build");

    // Two percent slow, so the p99 rank falls inside the slow group. One in a
    // hundred would not: with 100 samples the 99th is still a fast one, and a
    // p99 of 1 ms would be the correct answer to a different question.
    for _ in 0..980 {
        latency
            .record(Duration::from_millis(1))
            .expect("recordable");
    }
    for _ in 0..20 {
        latency
            .record(Duration::from_millis(500))
            .expect("recordable");
    }

    let p95 = latency.quantile_ms(P95);
    let p99 = latency.quantile_ms(P99);
    assert!(
        p95 < 10.0,
        "ninety-five percent of operations were fast, so p95 must stay fast, said {p95}"
    );
    assert!(
        p99 > 100.0,
        "two percent at half a second must show at p99, said {p99}"
    );
    assert!(
        (latency.max_ms() - 500.0).abs() < 500.0 * TOLERANCE,
        "the slowest operation must be reported as it was measured"
    );
}

#[test]
fn test_a_sub_millisecond_operation_is_not_rounded_away() {
    let mut latency = Latency::new().expect("the histogram must build");

    latency
        .record(Duration::from_micros(250))
        .expect("recordable");

    let reported = latency.quantile_ms(P95);
    assert!(
        reported > 0.0,
        "recording in microseconds is the reason a fast datastore does not \
         report a flat zero, said {reported}"
    );
    assert!((reported - 0.25).abs() < 0.25 * TOLERANCE);
}

#[test]
fn test_a_very_slow_operation_still_records() {
    let mut latency = Latency::new().expect("the histogram must build");

    latency
        .record(Duration::from_secs(600))
        .expect("a ten minute operation is a measurement, not an overflow");

    assert_eq!(latency.count(), 1);
}
