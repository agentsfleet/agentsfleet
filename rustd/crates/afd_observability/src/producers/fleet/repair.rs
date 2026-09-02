//! What the repair-verification pipeline records.
//!
//! A verification chain has five links: a provider result arrives, it
//! correlates to a repair, an intent is written, a dispatch takes it, and a
//! run reports back. This daemon owns the last two — the first three belong to
//! the repair-result ingress, which has no Rust home yet and whose families
//! say so by name in `crate::metrics::produced`.
//!
//! Every step counts separately, because the question an operator asks is
//! where the chain stopped, and a single end-to-end counter cannot answer it.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::{SyntheticEvent, VerifierRun};
use crate::producers::installed;
use crate::semconv;

/// Records a dispatch this pass had to try again.
pub fn dispatch_retried() {
    if let Some(producers) = installed() {
        producers.fleet.repair_retries.add(1, &[]);
    }
}

/// Records whether a verification event was appended or already there.
pub fn event(outcome: SyntheticEvent) {
    if let Some(producers) = installed() {
        producers.fleet.repair_events.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}

/// Records where a verification run got to.
pub fn run(outcome: VerifierRun) {
    if let Some(producers) = installed() {
        producers.fleet.repair_runs.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
