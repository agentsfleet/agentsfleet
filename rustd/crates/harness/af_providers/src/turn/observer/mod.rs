//! What one run's turns write to the host's journal and its measurements:
//! content-free lines under the run's id, failures under a registry code.

use std::fmt;
use std::time::Duration;

use af_core::clock::saturating_millis;
use af_wire::usage::ObservedTokens;
use guffbolt_ai::wire::{Calibration, FailureCause, Observer, Retrying, TurnObservation};

use crate::error::raise;
use crate::observe;
use crate::observe::labels::{CallOutcome, Provider, RetryReason};
use crate::transport::logged;

const STARTED: &str = "model_turn_started";
const FIRST_TEXT: &str = "model_turn_first_text";
const COMPLETED: &str = "model_turn_completed";
const FAILED: &str = "model_turn_failed";
const CALIBRATION: &str = "model_input_calibration";
/// The log line a retried send writes.
const EVENT_RETRY: &str = "provider_retry";
/// The log line a turn opened again writes.
const EVENT_REOPENED: &str = "provider_turn_reopened";

/// One run's observer: whose turns these are, for every line and
/// measurement they write.
pub(crate) struct RunObserver {
    run_id: Box<str>,
    /// The provider as a log line may name it: a `custom:` endpoint by its
    /// host alone.
    provider: Box<str>,
    /// The provider as the measurement families label it.
    label: Provider,
}

impl fmt::Debug for RunObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunObserver")
            .field("run_id", &self.run_id)
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl RunObserver {
    /// Run `run_id`'s observer, `provider` as the policy spells it.
    pub(crate) fn new(run_id: &str, provider: &str) -> Self {
        Self {
            run_id: run_id.into(),
            provider: logged(provider),
            label: Provider::of(provider),
        }
    }
}

/// The measurement outcome a turn's cause is.
const fn outcome(cause: Option<FailureCause>) -> CallOutcome {
    match cause {
        None => CallOutcome::Completed,
        Some(FailureCause::Provider) => CallOutcome::Provider,
        Some(FailureCause::Deadline) => CallOutcome::Deadline,
        Some(FailureCause::Cancelled) => CallOutcome::Cancelled,
    }
}

impl Observer for RunObserver {
    fn turn_started(&self) {
        let correlation_id = &*self.run_id;
        tracing::info!(event = STARTED, correlation_id);
    }

    fn first_text(&self, after: Duration) {
        let correlation_id = &*self.run_id;
        let duration_ms = saturating_millis(after);
        tracing::debug!(event = FIRST_TEXT, correlation_id, duration_ms);
    }

    fn turn_ended(&self, model: &str, turn: &TurnObservation) {
        observe::model_call(
            model,
            self.label,
            outcome(turn.cause),
            turn.elapsed,
            turn.first_text,
        );
        let tokens = ObservedTokens {
            input: turn.tokens.input,
            output: turn.tokens.output,
            cache_read: turn.tokens.cache_read,
            cache_write: turn.tokens.cache_write,
        };
        observe::model_tokens(model, self.label, tokens);
        let correlation_id = &*self.run_id;
        let duration_ms = saturating_millis(turn.elapsed);
        let text_bytes = turn.text_bytes;
        match turn.cause {
            None => tracing::info!(event = COMPLETED, correlation_id, duration_ms, text_bytes),
            Some(cause) => {
                let reason = cause.as_str();
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                let event = &FAILED;
                tracing::warn!(
                    event,
                    correlation_id,
                    duration_ms,
                    text_bytes,
                    reason,
                    error_code
                );
            }
        }
    }

    fn input_measured(&self, calibration: &Calibration<'_>) {
        let correlation_id = &*self.run_id;
        let Calibration {
            route_id,
            model_id,
            provider_api,
            request_kind,
            method,
            revision,
            measured_input_tokens,
            actual_input_tokens,
            measure_us,
        } = *calibration;
        let event = &CALIBRATION;
        tracing::info!(
            event,
            correlation_id,
            route_id,
            model_id,
            provider_api,
            request_kind,
            method,
            revision,
            measured_input_tokens,
            actual_input_tokens,
            measure_us
        );
    }

    fn retrying(&self, retry: Retrying) {
        observe::retry(self.label, RetryReason::of_status(retry.status));
        let run_id = &*self.run_id;
        let provider = &*self.provider;
        let status = retry.status;
        let attempt = retry.attempt;
        let wait_ms = saturating_millis(retry.wait);
        let event = EVENT_RETRY;
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            run_id, provider, status, attempt, wait_ms, event, error_code
        );
    }

    fn reopened(&self, attempt: usize, failure: &guffbolt_ai::Error) {
        observe::retry(self.label, RetryReason::StreamReopened);
        let run_id = &*self.run_id;
        let reason = failure.provider_failure().and_then(raise::code);
        let event = EVENT_REOPENED;
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(run_id, attempt, reason, event, error_code);
    }
}

#[cfg(test)]
mod tests;
