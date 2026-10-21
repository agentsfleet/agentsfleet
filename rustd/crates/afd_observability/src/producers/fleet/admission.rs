//! Outcomes of durable admission.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::AdmissionOutcome;
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
