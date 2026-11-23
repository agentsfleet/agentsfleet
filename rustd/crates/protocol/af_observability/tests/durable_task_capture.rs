//! Operation latency reaches the exporter under bounded, private-data-free labels.

use std::time::Duration;

use af_observability::{
    metrics::{
        declared::durable_task as declared,
        label::durable_task::{DurableTaskOperation, DurableTaskOutcome},
    },
    producers::durable_task,
    semconv,
    test_util::Capture,
};

#[test]
fn operation_latency_reaches_the_exporter_without_private_labels() {
    let capture = Capture::install();
    let operation = DurableTaskOperation::DecisionReserve;
    for (outcome, elapsed) in [
        (DurableTaskOutcome::Accepted, Duration::from_millis(4)),
        (DurableTaskOutcome::Failed, Duration::from_millis(12)),
        (DurableTaskOutcome::Cancelled, Duration::from_millis(80)),
        (DurableTaskOutcome::Timeout, Duration::from_secs(3)),
    ] {
        durable_task::operation_completed(operation, outcome, elapsed);
        let labels = [
            (semconv::LABEL_OPERATION, operation.as_str()),
            (semconv::LABEL_OUTCOME, outcome.as_str()),
        ];
        assert_eq!(
            capture.sum(declared::DURABLE_TASK_OPERATIONS_TOTAL.wire_name(), &labels),
            1
        );
        assert_eq!(
            capture.histogram_count(
                declared::DURABLE_TASK_OPERATION_SECONDS.wire_name(),
                &labels
            ),
            1
        );
        assert_eq!(
            capture.histogram_sum(
                declared::DURABLE_TASK_OPERATION_SECONDS.wire_name(),
                &labels
            ),
            elapsed.as_secs_f64()
        );
    }
}
