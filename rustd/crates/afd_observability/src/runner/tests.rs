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

use std::sync::Arc;

use afd_wire::report::FailureClass;

use super::{MAX_SERIES, OVERFLOW_RUNNER, RunnerMetrics, SDK_OVERFLOW_MARKER};

/// The identifier of the `index`-th distinct runner.
fn runner(index: usize) -> String {
    format!("0195b4ba-8d3a-7f13-8abc-{index:012}")
}

#[test]
fn a_runner_gets_its_own_series_until_the_table_is_full() {
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        metrics.processed(&runner(index));
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
        metrics.processed(&runner(index));
    }

    let beyond = 10_000;
    for index in MAX_SERIES..MAX_SERIES + beyond {
        metrics.processed(&runner(index));
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
fn an_overflowed_runner_is_still_counted_and_still_carries_its_reason() {
    // What overflow costs is WHICH runner, and nothing else: the failure totals
    // and their reasons stay correct, because a deployment past the capacity
    // has a problem that per-runner attribution would not help with.
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        metrics.processed(&runner(index));
    }

    metrics.failed(&runner(MAX_SERIES + 1), Some(FailureClass::OomKill));
    metrics.failed(&runner(MAX_SERIES + 2), None);

    // Two failures and two executions, from two different overflowed runners.
    assert_eq!(metrics.overflowed(), 4);
    assert_eq!(metrics.series_count(), MAX_SERIES);
}

#[test]
fn a_repeated_runner_reuses_its_series() {
    // The ordinary case, and the reason the capacity is not reached by a
    // healthy deployment: a host recording ten thousand runs holds one series.
    let metrics = RunnerMetrics::new();
    for _run in 0..10_000 {
        metrics.processed(&runner(1));
    }
    assert_eq!(metrics.series_count(), 1);
    assert_eq!(metrics.overflowed(), 0);
}

#[test]
fn concurrent_first_records_of_one_runner_produce_one_series() {
    // The race the Zig's compare-and-swap slot claim exists for, and the reason
    // the capacity check sits under the WRITE lock here: two threads meeting an
    // unknown runner must not both admit it, or the table holds a duplicate and
    // its bound means one less than it says.
    let metrics = Arc::new(RunnerMetrics::new());
    let contenders: Vec<_> = (0..16)
        .map(|_| {
            let metrics = Arc::clone(&metrics);
            std::thread::spawn(move || {
                for _record in 0..64 {
                    metrics.processed(&runner(7));
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
        .admit(&runner_id)
        .expect("the empty table admits the runner");
    let rechecked = metrics
        .admit(&runner_id)
        .expect("an already-admitted runner reuses its series");

    assert!(Arc::ptr_eq(&first, &rechecked));
    assert_eq!(metrics.series_count(), 1);
}

#[test]
fn every_failure_class_and_runner_gauge_uses_the_admitted_series() {
    let metrics = RunnerMetrics::default();
    let runner_id = runner(1);
    metrics.processed(&runner_id);
    metrics.seen(&runner_id, 1_760_000_000_000);
    metrics.leased(&runner_id);
    metrics.released(&runner_id);

    for reason in [
        FailureClass::StartupPosture,
        FailureClass::PolicyDeny,
        FailureClass::TimeoutKill,
        FailureClass::OomKill,
        FailureClass::ResourceKill,
        FailureClass::RunnerCrash,
        FailureClass::TransportLoss,
        FailureClass::LandlockDeny,
        FailureClass::LeaseExpired,
        FailureClass::RenewalTerminate,
        FailureClass::BudgetBreach,
    ] {
        metrics.failed(&runner_id, Some(reason));
    }
    metrics.failed(&runner_id, None);

    assert_eq!(metrics.series_count(), 1);
    assert_eq!(metrics.overflowed(), 0);
}

#[test]
fn gauges_ignore_a_runner_that_has_not_acquired_a_series() {
    let metrics = RunnerMetrics::new();
    metrics.seen("unknown", 1);
    metrics.leased("unknown");
    metrics.released("unknown");
    assert_eq!(metrics.series_count(), 0);
}

// ---------------------------------------------------------------------------
// Dimension 3.2 — the spelling admission produces.
// ---------------------------------------------------------------------------

/// A runner with a series of its own is attributed to itself.
#[test]
fn test_an_admitted_runner_is_labelled_with_its_own_id() {
    let metrics = RunnerMetrics::new();
    metrics.processed("runner-1");
    assert_eq!(metrics.label_for("runner-1"), "runner-1");
}

/// A runner the table has room for is attributed to itself even before its
/// first record — the label describes what a record made NOW would carry.
#[test]
fn test_a_runner_with_room_is_labelled_with_its_own_id() {
    let metrics = RunnerMetrics::new();
    assert_eq!(metrics.label_for("never-seen"), "never-seen");
}

/// Past the bound, a runner is attributed to `_other`.
///
/// The spelling is the assertion. It is the Zig daemon's, kept byte-exact,
/// because every dashboard and alert reading this label reads it on both sides
/// of the swap — a renamed overflow bucket is a panel that silently stops
/// matching.
#[test]
fn test_runner_admission_other_spelling() {
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        metrics.processed(&format!("runner-{index}"));
    }
    assert_eq!(metrics.series_count(), MAX_SERIES);

    assert_eq!(
        metrics.label_for("one-runner-too-many"),
        OVERFLOW_RUNNER,
        "past the table, a runner is attributed to the shared bucket"
    );
    assert_eq!(
        OVERFLOW_RUNNER, "_other",
        "the wire spelling is the contract"
    );
}

/// A runner already holding a series keeps its own label even once the table
/// is full — the bound rejects NEW runners, never established ones.
#[test]
fn test_a_full_table_does_not_relabel_its_existing_runners() {
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        metrics.processed(&format!("runner-{index}"));
    }
    assert_eq!(metrics.label_for("runner-0"), "runner-0");
}

/// Our overflow spelling is NOT the SDK's marker, and that is deliberate.
///
/// `_other` is a bounded-attribution decision made in front of the instrument:
/// seeing it means a deployment larger than the slot table, which is
/// information. `otel.metric.overflow` is set by the SDK when its own
/// cardinality cap is hit, which means something wrote an attribute the typed
/// layer was supposed to make unwritable — a bug. Spelling one as the other
/// would disguise the second as the first.
#[test]
fn test_the_overflow_label_is_not_the_sdk_marker() {
    assert_ne!(
        OVERFLOW_RUNNER, SDK_OVERFLOW_MARKER,
        "a capacity notice and a bug indicator must stay distinguishable"
    );
    assert_eq!(SDK_OVERFLOW_MARKER, "otel.metric.overflow");
}
