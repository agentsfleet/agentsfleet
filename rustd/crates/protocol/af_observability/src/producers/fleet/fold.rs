//! Where a steer went, and what became of the messages live runs took.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::{FoldOutcome, SteerPlaced};
use crate::producers::installed;
use crate::semconv;

/// Records one accepted steer, by where it went.
pub fn disposition(placed: SteerPlaced) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .steer_dispositions
            .add(1, &[KeyValue::new(semconv::LABEL_OUTCOME, placed.as_str())]);
    }
}

/// Records `messages` a live run's claim took, or its report answered.
pub fn folds(outcome: FoldOutcome, messages: u64) {
    if messages == 0 {
        return;
    }
    if let Some(producers) = installed() {
        producers.fleet.lease_folds.add(
            messages,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
