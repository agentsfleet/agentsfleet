//! Operation latency reaches the exporter under bounded, private-data-free labels.

use std::time::Duration;

use af_observability::{
    metrics::{
        declared::task as declared,
        label::task::{TaskOperation, TaskOutcome},
    },
    producers::task,
    semconv,
    test_util::Capture,
};

#[test]
fn operation_latency_reaches_the_exporter_without_private_labels() {
    let capture = Capture::install();
    let operation = TaskOperation::DecisionReserve;
    for (outcome, elapsed) in [
        (TaskOutcome::Accepted, Duration::from_millis(4)),
        (TaskOutcome::Failed, Duration::from_millis(12)),
        (TaskOutcome::Cancelled, Duration::from_millis(80)),
        (TaskOutcome::Timeout, Duration::from_secs(3)),
    ] {
        task::operation_completed(operation, outcome, elapsed);
        let labels = [
            (semconv::LABEL_OPERATION, operation.as_str()),
            (semconv::LABEL_OUTCOME, outcome.as_str()),
        ];
        assert_eq!(
            capture.sum(declared::TASK_OPERATIONS_TOTAL.wire_name(), &labels),
            1
        );
        assert_eq!(
            capture.histogram_count(declared::TASK_OPERATION_SECONDS.wire_name(), &labels),
            1
        );
        assert_eq!(
            capture.histogram_sum(declared::TASK_OPERATION_SECONDS.wire_name(), &labels),
            elapsed.as_secs_f64()
        );
    }
}
