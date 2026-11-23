//! Who is assigned a fleet whose thread named a held sandbox.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::HeldAssignment;
use crate::producers::installed;
use crate::semconv;

/// Records one won assignment on a held fleet, by who won it.
pub fn assigned(outcome: HeldAssignment) {
    if let Some(producers) = installed() {
        producers.fleet.held_assignments.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
