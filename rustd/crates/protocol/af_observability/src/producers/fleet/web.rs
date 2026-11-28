//! Counts a single physical vendor dispatch once it answers.

use opentelemetry::KeyValue;

use crate::producers::installed;
use crate::semconv;

/// Records a completed web dispatch using the daemon's fixed vendor, operation
/// and outcome names. URLs, queries and vendor responses never label a series.
pub fn dispatched(vendor: &'static str, operation: &'static str, outcome: &'static str) {
    if let Some(producers) = installed() {
        producers.fleet.web_calls.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_VENDOR, vendor),
                KeyValue::new(semconv::LABEL_OPERATION, operation),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome),
            ],
        );
    }
}
