//! One provider attempt, shared by private setup and fleet execution.

use std::sync::Arc;
use std::time::Duration;

mod trace;
use trace::TurnTrace;

use futures_util::StreamExt as _;
use rig_core::completion::{CompletionRequest, CompletionResponse};
use rig_core::operation::Completion;
use rig_core::streaming::{CompletionStream, Item, StreamEvent};
use rig_core::{DynModel, ProviderError};
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;

use crate::{Configured, Error, Result, document::Recovery, error::ErrorKind};

/// Shared connection establishment bound for guarded physical senders.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A silent provider cannot retain a worker indefinitely.
pub const READ_TIMEOUT: Duration = Duration::from_secs(300);
const CAUSE_FIELD: &str = "cause";

/// Caller-owned cancellation and total deadline; neither causes a second attempt.
#[derive(Debug, Clone, Default)]
pub struct TurnControl {
    /// Caller-owned dispatch identity; never a prompt, URL or credential.
    pub correlation_id: Arc<str>,
    /// The caller's absolute attempt deadline, when it has one.
    pub deadline: Option<Instant>,
    /// Cancels the physical response when the turn stops being polled.
    pub cancellation: CancellationToken,
}

#[cfg(test)]
mod tests;

/// Content-free classification; the original error remains in the final result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// The caller's hard deadline or the shared inactivity bound elapsed.
    Deadline,
    /// The caller cancelled the turn.
    Cancelled,
    /// The transport or provider ended the response unsuccessfully.
    Provider,
}

impl FailureCause {
    /// A bounded diagnostic word, independent of untrusted provider messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deadline => "deadline",
            Self::Cancelled => "cancelled",
            Self::Provider => "provider_interrupted",
        }
    }
}

/// A folded full or partial reply and the failure that prevents publication.
pub struct TurnResult {
    /// Partial replies retain authoritative usage received before an error.
    pub response: CompletionResponse,
    /// A partial reply must never be interpreted as successful completion.
    pub failure: Option<Box<ProviderError>>,
    /// Safe coarse cause for purpose-specific diagnostics.
    pub cause: Option<FailureCause>,
}

impl TurnResult {
    /// Runtime consumers require a completed reply, while accounting can inspect partial facts.
    ///
    /// # Errors
    /// Returns the original provider/transport error without discarding its source.
    pub fn into_response(self) -> Result<CompletionResponse, Box<ProviderError>> {
        self.failure.map_or(Ok(self.response), Err)
    }
}

/// Owns the SDK stream; provider codecs and event framing remain entirely in Rig.
pub struct StreamTurn {
    inner: CompletionStream,
    recovery: Recovery,
    trace: TurnTrace,
    control: TurnControl,
    failure: Option<Box<ProviderError>>,
    cause: Option<FailureCause>,
    ended: bool,
    last_event: Instant,
    measurement: crate::InputCount,
}

impl std::fmt::Debug for StreamTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTurn")
            .field("ended", &self.ended)
            .field(CAUSE_FIELD, &self.cause)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for TurnResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnResult")
            .field(CAUSE_FIELD, &self.cause)
            .finish_non_exhaustive()
    }
}

impl StreamTurn {
    /// Opens one SDK attempt. Retrying is an explicit caller policy outside this reader.
    ///
    /// # Errors
    /// Refuses a request the selected SDK wire cannot encode.
    pub fn open(
        configured: &Configured<'_>,
        model: &DynModel<Completion>,
        request: CompletionRequest,
        control: TurnControl,
    ) -> Result<Self, Box<ProviderError>> {
        let count = configured
            .input_count(
                request
                    .model
                    .as_deref()
                    .or_else(|| model.id())
                    .unwrap_or_default(),
                &request,
            )
            .map_err(|error| Box::new(ProviderError::from(error)))?;
        Self::open_counted(configured, model, request, count, control)
    }

    /// Opens the unchanged request associated with a fresh measurement.
    ///
    /// # Errors
    /// A changed request invalidates its count before the SDK can send it.
    pub fn open_counted(
        configured: &Configured<'_>,
        model: &DynModel<Completion>,
        request: CompletionRequest,
        count: crate::InputCount,
        control: TurnControl,
    ) -> Result<Self, Box<ProviderError>> {
        let model_name = request
            .model
            .as_deref()
            .or_else(|| model.id())
            .unwrap_or_default();
        configured
            .verify_count(model_name, &request, &count)
            .map_err(|error| Box::new(ProviderError::from(error)))?;
        let recovery = Recovery::new(configured, model_name);
        Ok(Self {
            inner: model.stream(request)?,
            recovery,
            trace: TurnTrace::start(Arc::clone(&control.correlation_id)),
            control,
            failure: None,
            cause: None,
            ended: false,
            last_event: Instant::now(),
            measurement: count,
        })
    }

    /// Reads one normalized event under shared cancellation, inactivity and total bounds.
    ///
    /// # Errors
    /// Yields one safe cause; `finish` retains the original error and any partial usage.
    pub async fn next(&mut self) -> Option<Result<StreamEvent, FailureCause>> {
        while !self.ended {
            let inactivity = self.last_event + READ_TIMEOUT;
            let deadline = self
                .control
                .deadline
                .map_or(inactivity, |end| end.min(inactivity));
            let read = tokio::select! {
                biased;
                () = self.control.cancellation.cancelled() => {
                    return Some(Err(self.stop(FailureCause::Cancelled, ErrorKind::StreamCancelled)));
                }
                read = timeout_at(deadline, self.inner.next()) => read,
            };
            match read {
                Ok(Some(Ok(Item::Event(event)))) => {
                    self.last_event = Instant::now();
                    if let StreamEvent::Text { text, .. } = &event {
                        self.trace.text(text.len());
                    }
                    return Some(Ok(event));
                }
                Ok(Some(Ok(Item::Unknown(_)))) => {
                    self.last_event = Instant::now();
                }
                Ok(Some(Err(error))) => {
                    self.failure = Some(Box::new(error));
                    self.cause = Some(FailureCause::Provider);
                    self.ended = true;
                    return Some(Err(FailureCause::Provider));
                }
                Ok(None) => self.ended = true,
                Err(_) => {
                    return Some(Err(
                        self.stop(FailureCause::Deadline, ErrorKind::StreamDeadline)
                    ));
                }
            }
        }
        None
    }

    fn stop(&mut self, cause: FailureCause, kind: ErrorKind) -> FailureCause {
        self.failure = Some(Box::new(Error::from(kind).into()));
        self.cause = Some(cause);
        self.ended = true;
        cause
    }

    /// Completes the fold, preserving partial usage even if reading or finalization failed.
    pub async fn finish(mut self) -> TurnResult {
        while self.next().await.is_some() {}
        let mut partial = self.inner.partial();
        let result = if let Some(failure) = self.failure {
            self.recovery.usage(&mut partial).await;
            self.trace.end(self.cause);
            TurnResult {
                response: partial,
                failure: Some(failure),
                cause: self.cause,
            }
        } else {
            finish_response(
                self.inner.finish().await,
                partial,
                self.recovery,
                self.trace,
            )
            .await
        };
        self.measurement
            .observe(&result.response.usage, &self.control.correlation_id);
        result
    }
}

async fn finish_response(
    finished: Result<CompletionResponse, ProviderError>,
    mut partial: CompletionResponse,
    recovery: Recovery,
    mut trace: TurnTrace,
) -> TurnResult {
    match finished {
        Ok(response) => {
            trace.end(None);
            TurnResult {
                response,
                failure: None,
                cause: None,
            }
        }
        Err(failure) => {
            recovery.usage(&mut partial).await;
            trace.end(Some(FailureCause::Provider));
            TurnResult {
                response: partial,
                failure: Some(Box::new(failure)),
                cause: Some(FailureCause::Provider),
            }
        }
    }
}
