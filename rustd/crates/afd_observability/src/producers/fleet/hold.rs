//! Who claims a fleet whose slot named a held sandbox.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::HeldClaim;
use crate::producers::installed;
use crate::semconv;

/// Records one won claim on a held fleet, by who won it.
pub fn claimed(outcome: HeldClaim) {
    if let Some(producers) = installed() {
        producers.fleet.held_claims.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
