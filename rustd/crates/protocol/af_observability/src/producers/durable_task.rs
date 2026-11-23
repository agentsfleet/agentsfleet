//! Task results are recorded after their guarded operation finishes.

use std::time::Duration;

use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram},
};

use crate::error::Result;
use crate::metrics::{
    declared::durable_task as declared,
    instrument::Instruments,
    label::durable_task::{DurableTaskOperation, DurableTaskOutcome},
};
use crate::{producers::installed, semconv};

/// Instruments owned by the task lifecycle producer.
#[derive(Debug)]
pub struct Handles {
    operations: Counter<u64>,
    latency: Histogram<f64>,
}

impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            operations: instruments.counter_u64(&declared::DURABLE_TASK_OPERATIONS_TOTAL)?,
            latency: instruments.histogram_f64(&declared::DURABLE_TASK_OPERATION_SECONDS)?,
        })
    }

    fn record(
        &self,
        operation: DurableTaskOperation,
        outcome: DurableTaskOutcome,
        elapsed: Duration,
    ) {
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
pub fn operation_completed(
    operation: DurableTaskOperation,
    outcome: DurableTaskOutcome,
    elapsed: Duration,
) {
    if let Some(producers) = installed() {
        producers.task.record(operation, outcome, elapsed);
    }
}
