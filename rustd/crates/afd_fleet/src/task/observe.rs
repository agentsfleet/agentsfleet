//! One measurement per native operation, with no raw proposal in diagnostics.

use std::time::Instant;

use afd_core::error_code;
use afd_observability::{
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
    let interrupted = scopeguard::guard(started, |started| {
        producers::task::operation_completed(operation, TaskOutcome::Cancelled, started.elapsed());
        tracing::info!(
            event = "dt_operation_cancelled",
            operation = operation.as_str(),
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
        tracing::warn!(event = "dt_operation_failed", operation = operation.as_str(),
            outcome = outcome.as_str(), error_code = %error.code(),
            reason = error.task_timeout_reason().or_else(|| error.decision_failure_reason()).unwrap_or("operation_failed"));
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
