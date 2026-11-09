//! Safe, finite failure fields; payloads and credential material never enter a log.

use crate::{Error, Gateway, Operation};

impl Gateway {
    pub(crate) fn observe(error: &Error, operation: Operation) {
        let error_code = error.code().as_str();
        let reason = error.reason();
        let detail = error.detail();
        let retryable = error.retryable();
        let operation = operation.as_str();
        tracing::warn!(
            event = "runner_stream_operation_failed",
            error_code,
            reason,
            detail = detail.as_ref(),
            retryable,
            operation
        );
    }
}
