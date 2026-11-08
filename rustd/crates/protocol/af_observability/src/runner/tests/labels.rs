//! The label admission answers: a runner's own identifier, or `_other` once
//! the table is full.
//!
//! Split from the parent for length; the capacity tests stay there.

use crate::runner::{MAX_SERIES, OVERFLOW_RUNNER, RunnerMetrics, SDK_OVERFLOW_MARKER};

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
/// The spelling is the assertion, because every dashboard and alert reading
/// this label matches on it — a renamed overflow bucket is a panel that
/// silently stops matching.
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
