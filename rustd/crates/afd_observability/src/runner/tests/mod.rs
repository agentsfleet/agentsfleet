//! Dimensions 6.4 and 3.2: a caller-supplied label cannot grow this table
//! without end, and what it overflows into is spelled deliberately.
//!
//! The property is memory, and memory is not directly assertable — so what is
//! asserted is its proxy and its cause: the series count never passes the
//! capacity however many distinct runners record, and everything past it lands
//! in ONE place that still counts.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

mod labels;
mod readings;

use std::sync::Arc;

use super::{MAX_SERIES, RunnerMetrics};

/// The identifier of the `index`-th distinct runner.
fn runner(index: usize) -> String {
    format!("0195b4ba-8d3a-7f13-8abc-{index:012}")
}

#[test]
fn a_runner_gets_its_own_series_until_the_table_is_full() {
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        let _admitted = metrics.admit(&runner(index));
    }
    assert_eq!(metrics.series_count(), MAX_SERIES);
    assert_eq!(metrics.overflowed(), 0, "the table was not yet full");
}

#[test]
fn past_the_capacity_everything_lands_in_one_series() {
    // The hostile case: a fleet re-enrolling in a loop produces an unbounded
    // stream of distinct runner ids. A registry that grew a series per id would
    // consume memory until the process died, with no request ever failing to
    // explain why.
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        let _admitted = metrics.admit(&runner(index));
    }

    let beyond = 10_000;
    for index in MAX_SERIES..MAX_SERIES + beyond {
        let _admitted = metrics.admit(&runner(index));
    }

    assert_eq!(
        metrics.series_count(),
        MAX_SERIES,
        "the table grew past its capacity"
    );
    assert_eq!(
        metrics.overflowed(),
        beyond as u64,
        "an overflowing record was dropped rather than counted"
    );
}

#[test]
fn an_overflowed_runner_is_counted_without_taking_a_series() {
    // What overflow costs is WHICH runner, and nothing else: the totals stay
    // correct, because a deployment past the capacity has a problem that
    // per-runner attribution would not help with.
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        let _admitted = metrics.admit(&runner(index));
    }

    let _admitted = metrics.admit(&runner(MAX_SERIES + 1));
    let _label = metrics.admit(&runner(MAX_SERIES + 2));

    // Two records, from two different overflowed runners, each admitted once.
    assert_eq!(metrics.overflowed(), 2);
    assert_eq!(metrics.series_count(), MAX_SERIES);
}

#[test]
fn a_repeated_runner_reuses_its_series() {
    // The ordinary case, and the reason the capacity is not reached by a
    // healthy deployment: a host recording ten thousand runs holds one series.
    let metrics = RunnerMetrics::new();
    for _run in 0..10_000 {
        let _admitted = metrics.admit(&runner(1));
    }
    assert_eq!(metrics.series_count(), 1);
    assert_eq!(metrics.overflowed(), 0);
}

#[test]
fn concurrent_first_records_of_one_runner_produce_one_series() {
    // The race the capacity check sits under the WRITE lock for: two threads
    // meeting an unknown runner must not both admit it, or the table holds a
    // duplicate and its bound means one less than it says.
    let metrics = Arc::new(RunnerMetrics::new());
    let contenders: Vec<_> = (0..16)
        .map(|_| {
            let metrics = Arc::clone(&metrics);
            std::thread::spawn(move || {
                for _record in 0..64 {
                    let _admitted = metrics.admit(&runner(7));
                }
            })
        })
        .collect();
    for contender in contenders {
        contender.join().expect("a contender ran to completion");
    }

    assert_eq!(metrics.series_count(), 1);
    assert_eq!(metrics.overflowed(), 0);
}

#[test]
fn admission_recheck_returns_the_series_that_won_the_race() {
    let metrics = RunnerMetrics::new();
    let runner_id = runner(7);
    let first = metrics
        .admit_new(&runner_id)
        .expect("the empty table admits the runner");
    let rechecked = metrics
        .admit_new(&runner_id)
        .expect("an already-admitted runner reuses its series");

    assert!(Arc::ptr_eq(&first, &rechecked));
    assert_eq!(metrics.series_count(), 1);
}
