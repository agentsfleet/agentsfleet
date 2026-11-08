//! Content-free timing, with a terminal record even when the caller drops a turn.

use std::sync::Arc;
use tokio::time::Instant;

use super::{FailureCause, TurnObservation, TurnObserver};

const STARTED: &str = "model_turn_started";
const FIRST_TEXT: &str = "model_turn_first_text";
const COMPLETED: &str = "model_turn_completed";
const FAILED: &str = "model_turn_failed";

pub(super) struct TurnTrace {
    correlation: Arc<str>,
    started: Instant,
    first_text: Option<std::time::Duration>,
    terminal: Option<(Instant, Option<FailureCause>)>,
    observer: Option<TurnObserver>,
    tokens: af_wire::usage::ObservedTokens,
    text_bytes: usize,
    ended: bool,
}

impl TurnTrace {
    pub(super) fn start(correlation: Arc<str>, observer: Option<TurnObserver>) -> Self {
        let correlation_id = correlation.as_ref();
        tracing::info!(event = STARTED, correlation_id);
        Self {
            correlation,
            started: Instant::now(),
            first_text: None,
            terminal: None,
            observer,
            tokens: af_wire::usage::ObservedTokens::default(),
            text_bytes: 0,
            ended: false,
        }
    }

    pub(super) fn text(&mut self, bytes: usize) {
        self.text_bytes = self.text_bytes.saturating_add(bytes);
        if bytes > 0 && self.first_text.is_none() {
            self.first_text = Some(self.started.elapsed());
            let correlation_id = self.correlation.as_ref();
            let duration_ms = af_core::clock::saturating_millis(self.started.elapsed());
            tracing::debug!(event = FIRST_TEXT, correlation_id, duration_ms);
        }
    }

    pub(super) const fn usage(&mut self, usage: &rig_core::completion::Usage) {
        self.tokens = af_wire::usage::ObservedTokens {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cached_input_tokens,
            cache_write: usage.cache_creation_input_tokens,
        };
    }

    pub(super) fn terminal(&mut self, cause: Option<FailureCause>) {
        let (_, known) = self.terminal.get_or_insert_with(|| (Instant::now(), cause));
        if known.is_none() {
            *known = cause;
        }
    }

    pub(super) fn end(&mut self, failure: Option<FailureCause>) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.terminal(failure);
        let (ended, failure) = self.terminal.unwrap_or((self.started, failure));
        let elapsed = ended.duration_since(self.started);
        if let Some(observer) = self.observer.take() {
            observer(TurnObservation {
                tokens: self.tokens,
                elapsed,
                first_text: self.first_text,
                cause: failure,
            });
        }
        let correlation_id = self.correlation.as_ref();
        let duration_ms = af_core::clock::saturating_millis(elapsed);
        let text_bytes = self.text_bytes;
        match failure {
            None => tracing::info!(event = COMPLETED, correlation_id, duration_ms, text_bytes),
            Some(cause) => {
                let reason = cause.as_str();
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
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
        let known = self
            .terminal
            .map_or(Some(FailureCause::Cancelled), |(_, cause)| cause);
        self.end(known);
    }
}
