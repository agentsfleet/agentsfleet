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

use super::{MAX_SERIES, OVERFLOW_RUNNER, RunnerMetrics, SDK_OVERFLOW_MARKER};

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

#[test]
fn a_runner_gauge_reads_only_the_admitted_series() {
    let metrics = RunnerMetrics::default();
    let runner_id = runner(1);
    let label = metrics.admit(&runner_id);
    assert_eq!(
        label, runner_id,
        "a table with room admits the runner itself"
    );

    metrics.seen(&runner_id, 1_760_000_000_000);
    metrics.leased(&runner_id);
    metrics.leased(&runner_id);
    metrics.released(&runner_id);

    let seen = metrics.last_seen_readings();
    assert_eq!(seen.len(), 1, "one admitted runner is one reading");
    let seen = seen.first().expect("the length was just asserted");
    assert_eq!(
        seen.value, 1_760_000_000,
        "the stamp is published in the seconds the census declares, not the \
         milliseconds it is stored in"
    );

    let leases = metrics.active_lease_readings();
    assert_eq!(leases.len(), 1);
    let leases = leases.first().expect("the length was just asserted");
    assert_eq!(leases.value, 1, "two taken and one given back is one held");

    assert_eq!(metrics.series_count(), 1);
    assert_eq!(metrics.overflowed(), 0);
}

/// A runner that never reported publishes no reading at all.
///
/// A zero would be published as `1970` by the last-seen gauge, which every
/// dashboard draws as the oldest host in the fleet — a gap is the truth.
#[test]
fn a_runner_never_heard_from_publishes_no_last_seen_reading() {
    let metrics = RunnerMetrics::new();
    let _label = metrics.admit(&runner(1));

    assert!(
        metrics.last_seen_readings().is_empty(),
        "an unpublished stamp must leave a gap, not report the epoch"
    );
    assert_eq!(
        metrics.active_lease_readings().len(),
        1,
        "a lease count of zero IS a measurement — the runner holds nothing"
    );
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
    let _label = metrics.admit("runner-1");
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
        let _admitted = metrics.admit(&format!("runner-{index}"));
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
        let _admitted = metrics.admit(&format!("runner-{index}"));
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

/// More releases than leases publishes no reading rather than a huge one.
///
/// The count is a signed cell and the reading is unsigned, so an over-release
/// is the one input that cannot be published truthfully. A conversion that
/// reached for `as` instead of `try_from` would report the largest number a
/// gauge can hold on a fleet that is merely idle.
#[test]
fn more_releases_than_leases_publishes_no_reading() {
    let metrics = RunnerMetrics::new();
    let runner_id = runner(1);
    let _label = metrics.admit(&runner_id);

    metrics.released(&runner_id);

    assert!(
        metrics.active_lease_readings().is_empty(),
        "a count below zero is a gap in the gauge, never a number in it"
    );
}

/// Each reading is attributed to the runner it was read from.
///
/// The values alone cannot catch a table that labels every reading with one
/// runner's identifier: two rows would still carry two values, and the graph
/// would draw both under whichever name won.
#[test]
fn a_reading_is_attributed_to_the_runner_it_was_read_from() {
    let metrics = RunnerMetrics::new();
    let (first, second) = (runner(1), runner(2));
    let _first = metrics.admit(&first);
    let _second = metrics.admit(&second);

    metrics.leased(&first);
    for _lease in 0..3 {
        metrics.leased(&second);
    }

    let mut attributed: Vec<(String, u64)> = metrics
        .active_lease_readings()
        .into_iter()
        .map(|reading| {
            let carried = reading
                .attributes
                .iter()
                .find(|pair| pair.key.as_str() == crate::semconv::LABEL_RUNNER_ID)
                .expect("every reading carries the runner it was read from");
            (carried.value.to_string(), reading.value)
        })
        .collect();
    attributed.sort();

    assert_eq!(attributed, vec![(first, 1), (second, 3)]);
}

/// A poisoned series lock reads empty rather than propagating the panic.
///
/// These readings are pulled by the metrics callback on the SDK's collection
/// cadence, on a thread that is not the one that recorded anything. A `RwLock`
/// poisons for the life of the process once any holder panics, so an `unwrap`
/// here would turn one unrelated panic into a permanent one — failing every
/// subsequent collection, on a path whose entire job is reporting.
///
/// Empty is the honest degradation: a gauge with no data point is a gap, which
/// is what this crate publishes for an unreadable cell everywhere else, rather
/// than a zero somebody would read as "no active leases".
#[test]
fn a_poisoned_series_lock_reads_empty_rather_than_panicking() {
    let metrics = Arc::new(RunnerMetrics::new());
    metrics.admit(&runner(0));
    assert!(
        !metrics.active_lease_readings().is_empty(),
        "the series is readable before anything poisons it"
    );

    let poisoner = Arc::clone(&metrics);
    let panicked = std::thread::spawn(move || {
        let _held = poisoner.series.write();
        // Ends by panicking, which is what poisons the lock this thread holds.
        // Spelled as a failing parse rather than `panic!` because the workspace
        // denies `clippy::panic` — and as a REAL failure rather than a literal
        // unwrap, which the lint set reads as a mistake rather than an intent.
        "not-a-number"
            .parse::<u64>()
            .expect("this thread ends by panicking, poisoning the lock it holds");
    })
    .join();
    assert!(panicked.is_err(), "the helper thread panicked as intended");

    assert!(
        metrics.active_lease_readings().is_empty(),
        "a poisoned lock publishes no data point instead of panicking the collector"
    );
}
