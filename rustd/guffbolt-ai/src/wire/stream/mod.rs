//! One provider attempt: rig's stream read under the caller's cancellation,
//! its total deadline and the idle bound, folded into one result.

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

use super::count::InputCount;
use super::document::Recovery;
use super::observe::{FailureCause, Observer};
use super::route::Route;
use super::timeouts::Timeouts;
use crate::Error;
use crate::error::{ErrorKind, Result};

/// A silent provider cannot retain a worker indefinitely: 300 s, the stream
/// idle default of the reference agent CLIs.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(300);
const CAUSE_FIELD: &str = "cause";

/// Caller-owned cancellation and total deadline; neither causes a second attempt.
#[derive(Clone, Default)]
pub(crate) struct TurnControl {
    /// Checked transport and semantic inactivity policy for this caller.
    pub(crate) timeouts: Timeouts,
    /// The caller's absolute attempt deadline, when it has one.
    pub(crate) deadline: Option<Instant>,
    /// Cancels the physical response when the turn stops being polled.
    pub(crate) cancellation: CancellationToken,
    /// Receives content-free timing once, including cancellation and failed streams.
    pub(crate) observer: Option<Arc<dyn Observer>>,
}

impl std::fmt::Debug for TurnControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnControl")
            .field("timeouts", &self.timeouts)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;

/// A folded full or partial reply and the failure that prevents publication.
pub(crate) struct TurnResult {
    /// Partial replies retain authoritative usage received before an error.
    pub(crate) response: CompletionResponse,
    /// A partial reply must never be interpreted as successful completion.
    pub(crate) failure: Option<Box<ProviderError>>,
    /// Safe coarse cause for purpose-specific diagnostics.
    pub(crate) cause: Option<FailureCause>,
}

impl TurnResult {
    /// Runtime consumers require a completed reply, while accounting can inspect partial facts.
    ///
    /// # Errors
    /// Returns the original provider/transport error without discarding its source.
    #[cfg(test)]
    pub(crate) fn into_response(self) -> Result<CompletionResponse, Box<ProviderError>> {
        self.failure.map_or(Ok(self.response), Err)
    }
}

/// Owns the SDK stream; provider codecs and event framing remain entirely in Rig.
pub(crate) struct StreamTurn {
    inner: Option<CompletionStream>,
    recovery: Recovery,
    trace: Option<TurnTrace>,
    control: TurnControl,
    failure: Option<Box<ProviderError>>,
    cause: Option<FailureCause>,
    ended: bool,
    last_event: Instant,
    measurement: InputCount,
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

#[expect(
    clippy::expect_used,
    reason = "only consuming finish takes the stream and trace, after its final next call"
)]
impl StreamTurn {
    /// Opens one SDK attempt. Retrying is an explicit caller policy outside this reader.
    ///
    /// # Errors
    /// Refuses a request the selected SDK wire cannot encode.
    #[cfg(test)]
    pub(crate) fn open(
        configured: &Route,
        model: &DynModel<Completion>,
        request: CompletionRequest,
        control: TurnControl,
    ) -> Result<Self> {
        let count = configured.input_count(
            request
                .model
                .as_deref()
                .or_else(|| model.id())
                .unwrap_or_default(),
            &request,
        )?;
        Self::open_counted(configured, model, request, count, control)
    }

    /// Opens the unchanged request associated with a fresh measurement.
    ///
    /// # Errors
    /// A changed request invalidates its count before the SDK can send it,
    /// and rig's own refusal to start the reply is the provider's failure.
    pub(crate) fn open_counted(
        configured: &Route,
        model: &DynModel<Completion>,
        request: CompletionRequest,
        count: InputCount,
        control: TurnControl,
    ) -> Result<Self> {
        let model_name = request
            .model
            .as_deref()
            .or_else(|| model.id())
            .unwrap_or_default();
        configured.verify_count(model_name, &request, &count)?;
        let recovery = Recovery::new(configured, model_name);
        let label: Box<str> = model_name.into();
        Ok(Self {
            inner: Some(model.stream(request).map_err(Error::provider)?),
            recovery,
            trace: Some(TurnTrace::start(&label, control.observer.clone())),
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
    ///
    /// # Panics
    /// An internal ownership invariant is violated; public calls cannot consume the fields early.
    pub(crate) async fn next(&mut self) -> Option<Result<StreamEvent, FailureCause>> {
        while !self.ended {
            let inactivity = self.last_event + self.control.timeouts.stream_idle();
            let deadline = self
                .control
                .deadline
                .map_or(inactivity, |end| end.min(inactivity));
            let read = tokio::select! {
                biased;
                () = self.control.cancellation.cancelled() => {
                    return Some(Err(self.stop(FailureCause::Cancelled, ErrorKind::StreamCancelled)));
                }
                read = timeout_at(deadline, self.inner.as_mut().expect("unfinished turn owns its stream").next()) => read,
            };
            match read {
                Ok(Some(Ok(Item::Event(event)))) => {
                    self.last_event = Instant::now();
                    if let StreamEvent::Text { text, .. } = &event {
                        self.trace
                            .as_mut()
                            .expect("unfinished turn owns its trace")
                            .text(text.len());
                    }
                    return Some(Ok(event));
                }
                Ok(Some(Ok(Item::Unknown(_)))) => {
                    self.last_event = Instant::now();
                }
                Ok(Some(Err(error))) => {
                    self.trace
                        .as_mut()
                        .expect("unfinished turn owns its trace")
                        .terminal(Some(FailureCause::Provider));
                    self.failure = Some(Box::new(error));
                    self.cause = Some(FailureCause::Provider);
                    self.ended = true;
                    return Some(Err(FailureCause::Provider));
                }
                Ok(None) => {
                    self.trace
                        .as_mut()
                        .expect("unfinished turn owns its trace")
                        .terminal(None);
                    self.ended = true;
                }
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
        self.trace
            .as_mut()
            .expect("unfinished turn owns its trace")
            .terminal(Some(cause));
        self.failure = Some(Box::new(Error::from(kind).into()));
        self.cause = Some(cause);
        self.ended = true;
        cause
    }

    /// Completes the fold, preserving partial usage even if reading or finalization failed.
    ///
    /// # Panics
    /// An internal ownership invariant is violated; consuming `self` prevents a second finish.
    pub(crate) async fn finish(mut self) -> TurnResult {
        while self.next().await.is_some() {}
        let inner = self.inner.take().expect("finish consumes its stream once");
        let mut trace = self.trace.take().expect("finish consumes its trace once");
        let mut partial = inner.partial();
        // Preserve known usage even if cancellation drops finish during an await.
        trace.usage(&partial.usage);
        let result = if let Some(failure) = self.failure.take() {
            self.recovery.usage(&mut partial).await;
            trace.usage(&partial.usage);
            trace.end(self.cause);
            TurnResult {
                response: partial,
                failure: Some(failure),
                cause: self.cause,
            }
        } else {
            finish_response(inner.finish().await, partial, &self.recovery, trace).await
        };
        self.measurement
            .observe(&result.response.usage, self.control.observer.as_deref());
        result
    }
}

impl Drop for StreamTurn {
    fn drop(&mut self) {
        // Read the SDK fold once on abandonment, without another provider call.
        // Normal finish takes both fields, so recovery usage cannot be overwritten.
        if let (Some(inner), Some(trace)) = (&self.inner, &mut self.trace) {
            trace.usage(&inner.partial().usage);
        }
    }
}

async fn finish_response(
    finished: Result<CompletionResponse, ProviderError>,
    mut partial: CompletionResponse,
    recovery: &Recovery,
    mut trace: TurnTrace,
) -> TurnResult {
    match finished {
        Ok(response) => {
            trace.usage(&response.usage);
            trace.end(None);
            TurnResult {
                response,
                failure: None,
                cause: None,
            }
        }
        Err(failure) => {
            trace.terminal(Some(FailureCause::Provider));
            recovery.usage(&mut partial).await;
            trace.usage(&partial.usage);
            trace.end(Some(FailureCause::Provider));
            TurnResult {
                response: partial,
                failure: Some(Box::new(failure)),
                cause: Some(FailureCause::Provider),
            }
        }
    }
}
