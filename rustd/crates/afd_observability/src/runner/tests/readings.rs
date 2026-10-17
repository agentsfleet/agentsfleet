//! What the gauges read: only admitted series, attributed to the runner they
//! were read from, and readable after a recorder panics.
//!
//! Split from the parent for length; the capacity tests stay there.

use std::sync::Arc;

use super::runner;
use crate::runner::RunnerMetrics;

#[test]
fn a_runner_gauge_reads_only_the_admitted_series() {
    let metrics = RunnerMetrics::default();
    let runner_id = runner(1);
    let label = metrics.admit(&runner_id);
    assert_eq!(
        label, runner_id,
        "a table with room admits the runner itself"
    );

    metrics.seen(
        &runner_id,
        jiff::Timestamp::constant(1_760_000_000, 999_000_000),
    );
    metrics.leased(&runner_id);
    metrics.leased(&runner_id);
    metrics.released(&runner_id);

    let seen = metrics.last_seen_readings();
    assert_eq!(seen.len(), 1, "one admitted runner is one reading");
    let seen = seen.first().expect("the length was just asserted");
    assert_eq!(
        seen.value, 1_760_000_000,
        "the stamp is published in the whole seconds the census declares, \
         truncated, not the milliseconds it is stored in"
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
    metrics.seen("unknown", jiff::Timestamp::UNIX_EPOCH);
    metrics.leased("unknown");
    metrics.released("unknown");
    assert_eq!(metrics.series_count(), 0);
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

/// A panic in one recorder leaves the table readable for everyone after it.
///
/// These readings are pulled by the metrics callback on the SDK's collection
/// cadence, on a thread that is not the one that recorded anything. The table
/// was a `RwLock<HashMap>` and this test asserted the degradation that shape
/// forced: a poisoned lock had to read EMPTY, because `RwLock` poisons for the
/// life of the process once any holder panics, and an `unwrap` there would
/// have turned one unrelated panic into a permanent one — failing every later
/// collection, on a path whose whole job is reporting.
///
/// A sharded map does not poison: a guard dropped during an unwind releases
/// its shard like any other. So the degradation is gone and the guarantee it
/// existed to protect is stronger — one panicking recorder costs nothing at
/// all, rather than costing every later reading. That is what is asserted
/// here, against the same sequence.
#[test]
fn a_panicking_recorder_leaves_the_table_readable() {
    let metrics = Arc::new(RunnerMetrics::new());
    let first = runner(0);
    metrics.admit(&first);
    metrics.leased(&first);

    let panicking = Arc::clone(&metrics);
    let second = runner(1);
    let recorded = second.clone();
    let panicked = std::thread::spawn(move || {
        panicking.admit(&recorded);
        panicking.leased(&recorded);
        // Ends by panicking, mid-way through recording. Spelled as a failing
        // parse rather than `panic!` because the workspace denies
        // `clippy::panic` — and as a REAL failure rather than a literal
        // unwrap, which the lint set reads as a mistake rather than an intent.
        "not-a-number"
            .parse::<u64>()
            .expect("this thread ends by panicking part-way through recording");
    })
    .join();
    assert!(panicked.is_err(), "the helper thread panicked as intended");

    let readings = metrics.active_lease_readings();
    assert_eq!(
        readings.len(),
        2,
        "both series stay readable after a recorder panics: {readings:?}"
    );
    assert_eq!(
        metrics.series_count(),
        2,
        "the panicking thread's series was admitted and is still counted"
    );
    metrics.leased(&first);
    assert!(
        metrics
            .active_lease_readings()
            .iter()
            .any(|reading| reading.value == 2),
        "the table still ACCEPTS records after the panic, not just reads"
    );
}
