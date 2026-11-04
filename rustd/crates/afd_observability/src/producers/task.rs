//! Task results are recorded after their guarded operation finishes.

use std::time::Duration;

use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram},
};

use crate::error::Result;
use crate::metrics::{
    declared::task as declared,
    instrument::Instruments,
    label::task::{DecisionRecoveryOutcome, TaskOperation, TaskOutcome},
};
use crate::{producers::installed, semconv};

/// Instruments owned by the task lifecycle producer.
#[derive(Debug)]
pub struct Handles {
    operations: Counter<u64>,
    latency: Histogram<f64>,
    recovery_attempts: Counter<u64>,
    recovery_overdue: Histogram<f64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            operations: instruments.counter_u64(&declared::TASK_OPERATIONS_TOTAL)?,
            latency: instruments.histogram_f64(&declared::TASK_OPERATION_SECONDS)?,
            recovery_attempts: instruments
                .counter_u64(&declared::DECISION_RECOVERY_ATTEMPTS_TOTAL)?,
            recovery_overdue: instruments
                .histogram_f64(&declared::DECISION_RECOVERY_OVERDUE_SECONDS)?,
        })
    }

    fn record(&self, operation: TaskOperation, outcome: TaskOutcome, elapsed: Duration) {
        let attributes = [
            KeyValue::new(semconv::LABEL_OPERATION, operation.as_str()),
            KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
        ];
        self.operations.add(1, &attributes);
        self.latency.record(elapsed.as_secs_f64(), &attributes);
    }
}

/// One completed native operation; transport and provider stages add their own
/// measurements at their existing owners rather than copying these counts.
pub fn operation_completed(operation: TaskOperation, outcome: TaskOutcome, elapsed: Duration) {
    if let Some(producers) = installed() {
        producers.task.record(operation, outcome, elapsed);
    }
}

/// One finished candidate attempt; retries remain visible as further attempts.
pub fn recovery_candidate_completed(outcome: DecisionRecoveryOutcome, overdue: Duration) {
    if let Some(producers) = installed() {
        let attributes = [KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())];
        producers.task.recovery_attempts.add(1, &attributes);
        producers
            .task
            .recovery_overdue
            .record(overdue.as_secs_f64(), &[]);
    }
}
