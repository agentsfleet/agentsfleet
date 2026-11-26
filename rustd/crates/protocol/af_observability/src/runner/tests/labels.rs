//! The label admission answers: a runner's own identifier, or `_other` once
//! the table is full.
//!
//! Split from the parent for length; the capacity tests stay there.

use crate::runner::{MAX_SERIES, OVERFLOW_RUNNER, RunnerMetrics};

/// Past the bound, a runner is attributed to `_other`.
///
/// The spelling is the assertion, because every dashboard and alert reading
/// this label matches on it — a renamed overflow bucket is a panel that
/// silently stops matching.
#[test]
fn test_runner_tracking_other_spelling() {
    let metrics = RunnerMetrics::new();
    for index in 0..MAX_SERIES {
        let _tracked = metrics.track(&format!("runner-{index}"));
    }
    assert_eq!(metrics.series_count(), MAX_SERIES);

    assert_eq!(
        OVERFLOW_RUNNER, "_other",
        "the wire spelling is the contract"
    );
}
