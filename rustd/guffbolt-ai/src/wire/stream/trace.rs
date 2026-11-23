//! Content-free timing, with a terminal observation even when the caller
//! drops a turn.

use std::sync::Arc;
use tokio::time::Instant;

use super::super::observe::{FailureCause, ObservedTokens, Observer, TurnObservation};

pub(super) struct TurnTrace {
    model: Box<str>,
    started: Instant,
    first_text: Option<std::time::Duration>,
    terminal: Option<(Instant, Option<FailureCause>)>,
    observer: Option<Arc<dyn Observer>>,
    tokens: ObservedTokens,
    text_bytes: usize,
    ended: bool,
}

impl TurnTrace {
    pub(super) fn start(model: &str, observer: Option<Arc<dyn Observer>>) -> Self {
        if let Some(observer) = &observer {
            observer.turn_started();
        }
        Self {
            model: model.into(),
            started: Instant::now(),
            first_text: None,
            terminal: None,
            observer,
            tokens: ObservedTokens::default(),
            text_bytes: 0,
            ended: false,
        }
    }

    pub(super) fn text(&mut self, bytes: usize) {
        self.text_bytes = self.text_bytes.saturating_add(bytes);
        if bytes > 0 && self.first_text.is_none() {
            let after = self.started.elapsed();
            self.first_text = Some(after);
            if let Some(observer) = &self.observer {
                observer.first_text(after);
            }
        }
    }

    pub(super) const fn usage(&mut self, usage: &rig_core::completion::Usage) {
        self.tokens = ObservedTokens {
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
        if let Some(observer) = self.observer.take() {
            observer.turn_ended(
                &self.model,
                &TurnObservation {
                    tokens: self.tokens,
                    elapsed: ended.duration_since(self.started),
                    first_text: self.first_text,
                    cause: failure,
                    text_bytes: self.text_bytes,
                },
            );
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
