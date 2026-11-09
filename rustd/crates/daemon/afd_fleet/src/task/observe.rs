//! One measurement per native operation, with no raw proposal in diagnostics.

use std::time::Instant;

use af_core::error_code;
use af_observability::{
    metrics::label::task::{TaskOperation, TaskOutcome},
    producers,
};

use crate::{Error, Result};

pub(super) async fn measure<T>(
    operation: TaskOperation,
    work: impl Future<Output = Result<T>>,
    success: impl FnOnce(&T) -> TaskOutcome,
) -> Result<T> {
    let started = Instant::now();
    let operation_label = operation.as_str();
    let interrupted = scopeguard::guard(started, |started| {
        producers::task::operation_completed(operation, TaskOutcome::Cancelled, started.elapsed());
        tracing::info!(
            event = "dt_operation_cancelled",
            operation = operation_label,
            reason = "operation_dropped"
        );
    });
    let result = work.await;
    let started = scopeguard::ScopeGuard::into_inner(interrupted);
    let outcome = match &result {
        Ok(value) => success(value),
        Err(error) => failure(error),
    };
    producers::task::operation_completed(operation, outcome, started.elapsed());
    if let Err(error) = &result {
        let outcome_label = outcome.as_str();
        let error_code = error.code();
        let reason = error
            .task_timeout_reason()
            .or_else(|| error.decision_failure_reason())
            .unwrap_or("operation_failed");
        tracing::warn!(event = "dt_operation_failed", operation = operation_label,
            outcome = outcome_label, %error_code, reason);
    }
    result
}

fn failure(error: &Error) -> TaskOutcome {
    if error.task_timeout_reason().is_some()
        || error.decision_failure_reason() == Some("decision_deadline")
    {
        return TaskOutcome::Timeout;
    }
    match error.code() {
        error_code::TASK_NOT_FOUND | error_code::RUN_LEASE_NOT_FOUND => TaskOutcome::NotFound,
        error_code::TASK_STALE_STATE
        | error_code::TASK_PRECONDITION_FAILED
        | error_code::RUN_STALE_FENCING_TOKEN
        | error_code::RUN_LEASE_LOST => TaskOutcome::Stale,
        error_code::TASK_INVALID_TARGET
        | error_code::TASK_INPUT_AMBIGUOUS
        | error_code::TASK_CHECKPOINT_INCOMPATIBLE => TaskOutcome::Invalid,
        error_code::TASK_LIMIT_EXHAUSTED => TaskOutcome::Limit,
        error_code::TASK_DECISION_UNAVAILABLE | error_code::TASK_STORAGE_UNAVAILABLE => {
            TaskOutcome::Unavailable
        }
        error_code::TASK_EFFECT_UNCERTAIN if error.remote_cooldown().is_some() => {
            TaskOutcome::Unavailable
        }
        error_code::TASK_REPLAY_CONFLICT | error_code::TASK_EFFECT_UNCERTAIN => {
            TaskOutcome::Conflict
        }
        _ if error.is_datastore_unavailable() => TaskOutcome::Unavailable,
        _ => TaskOutcome::Failed,
    }
}

/// Box the measured operation at the lifecycle boundary, where object-safe dispatch requires it.
pub(super) fn call<'a, T: 'a>(
    operation: TaskOperation,
    work: impl Future<Output = Result<T>> + Send + 'a,
) -> super::TaskFuture<'a, T> {
    Box::pin(measure(operation, work, |_| TaskOutcome::Accepted))
}

/// Keep the operation's span attached for every poll, including cancellation and completion.
pub(super) fn traced<'a, T: 'a>(
    operation: TaskOperation,
    work: impl Future<Output = Result<T>> + Send + 'a,
    success: impl FnOnce(&T) -> TaskOutcome + Send + 'a,
    span: tracing::Span,
) -> super::TaskFuture<'a, T> {
    use tracing::Instrument as _;
    Box::pin(measure(operation, work, success).instrument(span))
}

/// Record an accepted successful result while retaining its original operation span.
pub(super) fn trace<'a, T: 'a>(
    operation: TaskOperation,
    work: impl Future<Output = Result<T>> + Send + 'a,
    span: tracing::Span,
) -> super::TaskFuture<'a, T> {
    traced(operation, work, |_| TaskOutcome::Accepted, span)
}
