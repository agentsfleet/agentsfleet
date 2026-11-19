//! One model turn streamed through rig into the seam's chunks.
//!
//! Text and reasoning go out as they arrive; a tool call goes out whole once
//! its arguments end; when the stream is done, the turn's usage and its end
//! follow: the reasoning the provider wants back, and whether the turn stopped
//! at its output limit. A call the provider sent with no id is given one, so
//! the result it is answered with still matches.
//!
//! A turn the provider ends early, after its reply began but before anything
//! visible went out, is opened again under the same bound the transport's
//! retry keeps, the rule `IronClaw`'s streaming retry follows
//! (`ironclaw_llm/src/retry.rs`). Once text or a call has gone out live, a
//! failure ends the turn: a second pass would show it twice.
//!
//! A malformed call goes out with its raw argument text, so the tool refuses
//! it with a reason the model reads. Other calls and usage still arrive.

use std::fmt;

mod progress;
use progress::Pass;

use crate::observe;
use crate::observe::labels::{CallOutcome, Provider as ProviderLabel, RetryReason};
use af_inference::stream::{StreamTurn, TurnControl};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use guffbolt::{Call, Chunk, End, Model, Replay, Request, TextKind};
use rig_core::DynModel;
use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::error::ProviderError;
use rig_core::message::{AssistantContent, CallId};
use rig_core::operation::Completion;
use rig_core::streaming::StreamEvent;

use crate::error::{Error, Result, raise};
use crate::request;
use crate::usage;
use af_crypto::secret::SecretString;
use af_inference::transport::HttpTransport;
use af_wire::inference::{InferenceApi, ModelInference};
use af_wire::policy::ExecutionPolicy;

/// The log line a turn opened again writes.
const EVENT_REOPENED: &str = "provider_turn_reopened";

/// One run's provider: a rig model and the wire it speaks.
pub(crate) struct Turns {
    model: DynModel<Completion>,
    settings: ModelInference,
    model_context_tokens: u32,
    run_id: Box<str>,
    /// The provider as the policy spells it, named when its endpoint is refused.
    provider: Box<str>,
    counting_key: SecretString,
    counting_transport: HttpTransport,
    timeouts: af_inference::Timeouts,
    retries: crate::RetryPolicy,
}

impl fmt::Debug for Turns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Turns")
            .field("wire", &self.settings.api)
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl Turns {
    /// Run `run_id`'s provider, `provider` as the policy spells it,
    /// using the selected model's persisted inference and context settings.
    pub(crate) fn new(
        model: DynModel<Completion>,
        settings: &ModelInference,
        policy: &ExecutionPolicy<'_>,
        run_id: &str,
        counting_transport: HttpTransport,
    ) -> Self {
        Self {
            model,
            settings: settings.clone(),
            model_context_tokens: policy.model_context_tokens,
            run_id: run_id.into(),
            provider: policy.provider.as_ref().into(),
            counting_key: SecretString::new(policy.api_key.to_string()),
            counting_transport,
            timeouts: af_inference::Timeouts::DEFAULT,
            retries: crate::RetryPolicy::DEFAULT,
        }
    }

    pub(crate) const fn with_settings(
        mut self,
        timeouts: af_inference::Timeouts,
        retries: crate::RetryPolicy,
    ) -> Self {
        self.timeouts = timeouts;
        self.retries = retries;
        self
    }

    /// `failure` as the kind a report names. An endpoint the client's guard
    /// refused is the policy's error, named as such, never a lost connection.
    fn failed(&self, failure: ProviderError) -> Error {
        if raise::blocked(&failure) {
            raise::blocked_endpoint(&self.provider)
        } else {
            raise::provider(failure)
        }
    }

    /// Preserve the provider's call identity or the local identity issued by rig.
    fn call(id: &CallId, name: String, arguments: serde_json::Value) -> Call {
        Call {
            id: id.wire().into_owned(),
            name,
            arguments,
        }
    }

    /// Opens the turn `request` asks for.
    ///
    /// # Errors
    /// The conversation cannot be sent, or rig could not start the reply.
    async fn open(&self, request: &Request<'_>) -> Result<StreamTurn> {
        let built = request::request(self.settings.api, request)?;
        let route = af_inference::Configured::new(&self.settings, self.model_context_tokens)
            .map_err(raise::configuration)?;
        let observed_model = request.model.to_owned();
        let observed_provider = ProviderLabel::of(&self.provider);
        let control = TurnControl {
            timeouts: self.timeouts,
            observer: Some(std::sync::Arc::new(move |timing| {
                use af_inference::stream::FailureCause;
                let outcome = match timing.cause {
                    None => CallOutcome::Completed,
                    Some(FailureCause::Provider) => CallOutcome::Provider,
                    Some(FailureCause::Deadline) => CallOutcome::Deadline,
                    Some(FailureCause::Cancelled) => CallOutcome::Cancelled,
                };
                observe::model_call(
                    &observed_model,
                    observed_provider,
                    outcome,
                    timing.elapsed,
                    timing.first_text,
                );
                observe::model_tokens(&observed_model, observed_provider, timing.tokens);
            })),
            correlation_id: self.run_id.clone().into(),
            ..Default::default()
        };
        let mut count = route
            .confirm_input(
                request.model,
                self.counting_key.expose(),
                &built,
                &self.counting_transport,
                &control,
            )
            .await
            .map_err(|error| self.failed(error.into()))?;
        let built = route
            .bound_counted_output(request.model, built, &mut count)
            .map_err(raise::configuration)?;
        StreamTurn::open_counted(&route, &self.model, built, count, control)
            .map_err(|failure| self.failed(*failure))
    }

    /// Logs and counts a turn opened again after `failure`.
    fn reopened(&self, attempt: usize, failure: &ProviderError) {
        observe::retry(
            ProviderLabel::of(&self.provider),
            RetryReason::StreamReopened,
        );
        let run_id = &*self.run_id;
        let reason = raise::code(failure);
        let event = EVENT_REOPENED;
        let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(run_id, attempt, reason, event, error_code);
    }
}

#[cfg(test)]
mod tests;

impl Model for Turns {
    type Error = Error;

    fn measure(&self, request: Request<'_>) -> Result<u64> {
        let body = request::request(self.settings.api, &request)?;
        let route = af_inference::Configured::new(&self.settings, self.model_context_tokens)
            .map_err(raise::configuration)?;
        route
            .measure_input(request.model, &body)
            .map_err(raise::configuration)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>> {
        stream::unfold(Pass::Opening(1), move |pass| pass.next(self, request)).boxed()
    }

    fn accepts_images(&self) -> bool {
        self.settings.api.carries_images()
    }
}

/// The chunks a turn that ended still has to send: what it spent, then its
/// end.
type Ending = std::array::IntoIter<Result<Chunk>, 2>;

/// The chunk one event is, when it is one the seam carries.
fn chunk(event: StreamEvent) -> Option<Chunk> {
    match event {
        StreamEvent::Text { text, .. } => Some(Chunk::Text {
            kind: TextKind::Answer,
            text,
        }),
        StreamEvent::Reasoning { text, .. } => Some(Chunk::Text {
            kind: TextKind::Reasoning,
            text,
        }),
        StreamEvent::End {
            content: AssistantContent::ToolCall(call),
            ..
        } => {
            let name = String::from(call.function.name);
            Some(Chunk::Call(Turns::call(
                &call.id,
                name,
                call.function.invalid_arguments.map_or_else(
                    || serde_json::Value::Object(call.function.arguments),
                    serde_json::Value::String,
                ),
            )))
        }
        StreamEvent::Start { .. } | StreamEvent::Arguments { .. } | StreamEvent::End { .. } => None,
    }
}

/// Whether a failure mid-stream may pass on a second opening: one the
/// provider sent after its reply began, with no status of its own. A reply
/// the transport ended at its cap is not one: the next would be as long. Nor
/// is an endpoint the guard refused: its name resolves the same way again.
fn reopens(failure: &ProviderError) -> bool {
    failure.provider_response().is_none()
        && failure.is_retryable()
        && !raise::oversize(failure)
        && !raise::blocked(failure)
}

/// The chunks a finished turn on `api` ends on: what it spent, then its end.
/// The end carries the turn's own message as its replay; one that will not
/// encode fails the turn after its spend is sent, never drops the record.
fn ending(api: InferenceApi, response: CompletionResponse) -> Ending {
    let cut = !matches!(
        response.finish_reason(),
        Some(FinishReason::Stop | FinishReason::ToolCalls)
    );
    let spent = usage::spent(api, &response.usage, &response.raw);
    let message = response.head().with_content(response.choice);
    let end = serde_json::to_value(&message)
        .map(|record| {
            Chunk::End(End {
                replay: Replay::new(record),
                cut,
            })
        })
        .map_err(Error::from);
    [Ok(Chunk::Usage(spent)), end].into_iter()
}
