//! Content-free compaction spans, paired even when an async caller is dropped.

use std::{sync::Arc, time::Instant};

use afd_core::error_code::{self, ErrorCode};
use tracing::Span;

use crate::Compaction;

const STARTED: &str = "context_compaction_started";
const COMPLETED: &str = "context_compaction_completed";
const FAILED: &str = "context_compaction_failed";

/// The production host, independent of customer content or model names.
#[derive(Debug, Clone, Copy)]
pub enum Host {
    /// Creator-private setup conversation.
    Authoring,
    /// One fleet execution branch.
    Runtime,
}

/// One physical summary and its guarded checkpoint publication.
#[derive(Debug)]
pub struct Trace {
    span: Span,
    correlation: Arc<str>,
    started: Instant,
    ended: bool,
}

impl Trace {
    /// Starts after preparation; identifiers must be caller-owned operation IDs.
    #[must_use]
    pub fn start(correlation: Arc<str>, host: Host, task: &Compaction) -> Self {
        let host = match host {
            Host::Authoring => "authoring",
            Host::Runtime => "runtime",
        };
        let correlation_id = correlation.as_ref();
        let expected_generation = task.expected_generation;
        let group_count = task.groups.len();
        let reserved_output_tokens = task.output_tokens;
        let span = tracing::info_span!(
            "context.compact",
            host,
            correlation_id,
            expected_generation,
            group_count,
            reserved_output_tokens,
            generation = tracing::field::Empty
        );
        span.in_scope(|| tracing::info!(event = STARTED, correlation_id));
        Self {
            span,
            correlation,
            started: Instant::now(),
            ended: false,
        }
    }

    /// Instrument the future rather than retaining an entered span across await.
    #[must_use]
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// Called only after the durable transaction or checkpoint commit succeeds.
    pub fn completed(&mut self, generation: u64) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.span.record("generation", generation);
        let correlation_id = self.correlation.as_ref();
        let duration_ms = afd_core::clock::saturating_millis(self.started.elapsed());
        self.span.in_scope(|| {
            tracing::info!(event = COMPLETED, correlation_id, duration_ms, generation);
        });
    }

    /// The registry code is retained; private error messages are never formatted.
    pub fn failed(&mut self, code: ErrorCode) {
        self.finish_failed(code, "operation_failed");
    }

    fn finish_failed(&mut self, code: ErrorCode, reason: &'static str) {
        if self.ended {
            return;
        }
        self.ended = true;
        let correlation_id = self.correlation.as_ref();
        let duration_ms = afd_core::clock::saturating_millis(self.started.elapsed());
        let error_code = code.as_str();
        self.span.in_scope(|| {
            tracing::warn!(
                event = FAILED,
                correlation_id,
                duration_ms,
                error_code,
                reason
            );
        });
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        self.finish_failed(
            error_code::INTERNAL_OPERATION_FAILED,
            "cancelled_or_dropped",
        );
    }
}
