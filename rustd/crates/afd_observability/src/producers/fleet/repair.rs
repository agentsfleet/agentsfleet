//! What the repair-verification pipeline records.
//!
//! One inbound provider result becomes a chain: it is accepted or ignored, it
//! correlates to a repair or does not, an intent is written, a dispatch takes
//! it, and a verification run reports back. Every step counts, because the
//! question an operator asks is where the chain stopped — and a single
//! end-to-end counter cannot answer it.

use std::time::Duration;

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::{Correlation, ProviderResult, SyntheticEvent, VerifierRun};
use crate::producers::installed;
use crate::semconv;

/// Records what became of one inbound provider result.
pub fn repair_provider_result(result: ProviderResult) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .repair_provider_results
            .add(1, &[KeyValue::new(semconv::LABEL_OUTCOME, result.as_str())]);
    }
}

/// Records whether a result could be tied to the repair that caused it.
pub fn repair_correlated(correlation: Correlation) {
    if let Some(producers) = installed() {
        producers.fleet.repair_correlations.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, correlation.as_str())],
        );
    }
}

/// Records a durable verification intent.
pub fn repair_intent_created() {
    if let Some(producers) = installed() {
        producers.fleet.repair_intents.add(1, &[]);
    }
}

/// Records a dispatch this pass had to try again.
pub fn repair_dispatch_retried() {
    if let Some(producers) = installed() {
        producers.fleet.repair_retries.add(1, &[]);
    }
}

/// Records whether a verification event was appended or already there.
pub fn repair_event(outcome: SyntheticEvent) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .repair_events
            .add(1, &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())]);
    }
}

/// Records where a verification run got to.
pub fn repair_run(outcome: VerifierRun) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .repair_runs
            .add(1, &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())]);
    }
}

/// Records how long a repair waited between production and being queued.
pub fn repair_production_to_queue(waited: Duration) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .repair_to_queue
            .record(waited.as_secs_f64(), &[]);
    }
}

/// Records how long a queued repair took to complete.
pub fn repair_queue_to_completion(waited: Duration) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .repair_to_completion
            .record(waited.as_secs_f64(), &[]);
    }
}
