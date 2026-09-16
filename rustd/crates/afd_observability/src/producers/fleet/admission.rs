//! What the admission ledger records.
//!
//! Two counters, and they answer different questions. The first says what
//! happened to work as it arrived; the second says how much of it the queue
//! made the sweeper come back for. A deployment where the second is
//! persistently non-zero has a queue that is not keeping up with its
//! acceptances, which is a different incident from one that refuses them.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::{AdmissionOutcome, ReplayOutcome};
use crate::producers::installed;
use crate::semconv;

/// Records what became of one admission.
pub fn admitted(outcome: AdmissionOutcome) {
    if let Some(producers) = installed() {
        producers.fleet.admissions.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}

/// Records what became of one replayed admission.
pub fn replayed(outcome: ReplayOutcome) {
    if let Some(producers) = installed() {
        producers.fleet.admission_replays.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
