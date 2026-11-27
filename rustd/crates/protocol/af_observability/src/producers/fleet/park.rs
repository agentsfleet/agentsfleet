//! Who is assigned a fleet whose thread named a parked sandbox.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::ParkedAssignment;
use crate::producers::installed;
use crate::semconv;

/// Records one won assignment on a parked fleet, by who won it.
pub fn assigned(outcome: ParkedAssignment) {
    if let Some(producers) = installed() {
        producers.fleet.parked_assignments.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
