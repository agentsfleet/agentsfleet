//! Content-free timing, with a terminal record even when the caller drops a turn.

use std::sync::Arc;
use tokio::time::Instant;

use super::FailureCause;

const STARTED: &str = "model_turn_started";
const FIRST_TEXT: &str = "model_turn_first_text";
const COMPLETED: &str = "model_turn_completed";
const FAILED: &str = "model_turn_failed";

pub(super) struct TurnTrace {
    correlation: Arc<str>,
    started: Instant,
    first_text: bool,
    text_bytes: usize,
    ended: bool,
}

impl TurnTrace {
    pub(super) fn start(correlation: Arc<str>) -> Self {
        let correlation_id = correlation.as_ref();
        tracing::info!(event = STARTED, correlation_id);
        Self {
            correlation,
            started: Instant::now(),
            first_text: false,
            text_bytes: 0,
            ended: false,
        }
    }

    pub(super) fn text(&mut self, bytes: usize) {
        self.text_bytes = self.text_bytes.saturating_add(bytes);
        if bytes > 0 && !self.first_text {
            self.first_text = true;
            let correlation_id = self.correlation.as_ref();
            let duration_ms = afd_core::clock::saturating_millis(self.started.elapsed());
            tracing::debug!(event = FIRST_TEXT, correlation_id, duration_ms);
        }
    }

    pub(super) fn end(&mut self, failure: Option<FailureCause>) {
        if self.ended {
            return;
        }
        self.ended = true;
        let correlation_id = self.correlation.as_ref();
        let duration_ms = afd_core::clock::saturating_millis(self.started.elapsed());
        let text_bytes = self.text_bytes;
        match failure {
            None => tracing::info!(event = COMPLETED, correlation_id, duration_ms, text_bytes),
            Some(cause) => {
                let reason = cause.as_str();
                let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(
                    event = FAILED,
                    correlation_id,
                    duration_ms,
                    text_bytes,
                    reason,
                    error_code
                );
            }
        }
    }
}

impl Drop for TurnTrace {
    fn drop(&mut self) {
        self.end(Some(FailureCause::Cancelled));
    }
}
