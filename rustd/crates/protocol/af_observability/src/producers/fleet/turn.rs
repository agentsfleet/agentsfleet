//! Outcomes of durable turn intake.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::IncomingTurnOutcome;
use crate::producers::installed;
use crate::semconv;

/// Records what became of one incoming turn.
pub fn admitted(outcome: IncomingTurnOutcome) {
    if let Some(producers) = installed() {
        producers.fleet.turns.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
