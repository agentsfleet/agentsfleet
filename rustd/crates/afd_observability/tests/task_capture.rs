//! Recovery and latency reach the exporter under bounded, private-data-free labels.

use std::time::Duration;

use afd_observability::{
    metrics::{
        declared::task as declared,
        label::task::{DecisionRecoveryOutcome as Recovery, TaskOperation, TaskOutcome},
    },
    producers::task,
    semconv,
    test_util::Capture,
};

#[test]
fn recovery_outcomes_and_operation_latency_reach_the_exporter_without_private_labels() {
    let capture = Capture::install();
    let attempts = declared::DECISION_RECOVERY_ATTEMPTS_TOTAL.wire_name();
    for (outcome, count, seconds) in [
        (Recovery::Expired, 1, 2),
        (Recovery::Obsolete, 2, 3),
        (Recovery::Contended, 3, 5),
        (Recovery::Failed, 4, 7),
    ] {
        for _ in 0..count {
            task::recovery_candidate_completed(outcome, Duration::from_secs(seconds));
        }
        assert_eq!(
            capture.sum(attempts, &[(semconv::LABEL_OUTCOME, outcome.as_str())]),
            count
        );
    }
    let overdue = declared::DECISION_RECOVERY_OVERDUE_SECONDS.wire_name();
    assert_eq!(capture.histogram_count(overdue, &[]), 10);
    assert_eq!(capture.histogram_sum(overdue, &[]), 51.0);
    operations(&capture);
}

fn operations(capture: &Capture) {
    let operation = TaskOperation::DecisionSweep;
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
