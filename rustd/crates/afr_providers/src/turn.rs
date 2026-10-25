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
use std::ops::ControlFlow;

use afd_inference::stream::{StreamTurn, TurnControl};
use afd_wire::activity::StreamTextKind;
use afr_telemetry::labels::{Provider as ProviderLabel, RetryReason};
use afr_telemetry::record;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use rig_core::DynModel;
use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::error::ProviderError;
use rig_core::message::{AssistantContent, CallId};
use rig_core::operation::Completion;
use rig_core::streaming::StreamEvent;

use crate::error::{Error, Result, raise};
use crate::provider::{Call, Chunk, End, Provider, Replay, Request};
use crate::request;
use crate::retry::ATTEMPTS;
use crate::usage;
use afd_crypto::secret::SecretString;
use afd_inference::transport::HttpTransport;
use afd_wire::inference::{InferenceApi, ModelInference};
use afd_wire::policy::ExecutionPolicy;

/// The log line a turn opened again writes.
const EVENT_REOPENED: &str = "provider_turn_reopened";

/// One lease's provider: a rig model and the wire it speaks.
pub(crate) struct Turns {
    model: DynModel<Completion>,
    settings: ModelInference,
    model_context_tokens: u32,
    lease_id: Box<str>,
    /// The provider as the policy spells it, named when its endpoint is refused.
    provider: Box<str>,
    counting_key: SecretString,
    counting_transport: HttpTransport,
}

impl fmt::Debug for Turns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Turns")
            .field("wire", &self.settings.api)
            .field("lease_id", &self.lease_id)
            .finish_non_exhaustive()
    }
}

impl Turns {
    /// Lease `lease_id`'s provider, `provider` as the policy spells it,
    /// using the selected model's persisted inference and context settings.
    pub(crate) fn new(
        model: DynModel<Completion>,
        settings: &ModelInference,
        policy: &ExecutionPolicy<'_>,
        lease_id: &str,
        counting_transport: HttpTransport,
    ) -> Self {
        Self {
            model,
            settings: settings.clone(),
            model_context_tokens: policy.model_context_tokens,
            lease_id: lease_id.into(),
            provider: policy.provider.as_ref().into(),
            counting_key: SecretString::new(policy.api_key.to_string()),
            counting_transport,
        }
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
        let route = afd_inference::Configured::new(&self.settings, self.model_context_tokens)
            .map_err(raise::configuration)?;
        let control = TurnControl {
            correlation_id: self.lease_id.clone().into(),
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
        record::retry(
            ProviderLabel::of(&self.provider),
            RetryReason::StreamReopened,
        );
        let lease_id = &*self.lease_id;
        let reason = raise::code(failure);
        let event = EVENT_REOPENED;
        let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(lease_id, attempt, reason, event, error_code);
    }
}

#[cfg(test)]
#[path = "turn_tests.rs"]
mod tests;

impl Provider for Turns {
    fn measure(&self, request: Request<'_>) -> Result<u64> {
        let body = request::request(self.settings.api, &request)?;
        let route = afd_inference::Configured::new(&self.settings, self.model_context_tokens)
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

/// Where a turn's stream is.
enum Pass {
    /// About to open the stream, on this attempt. The request is built each
    /// opening from the borrowed conversation, so the common turn, opened
    /// once, holds no second copy of it for a reopening that never comes.
    Opening(usize),
    /// Allocated once per opening; chunks retain the same reader allocation.
    Reading(Box<Reading>),
    /// What it spent and how it ended, still to go out.
    Ending(Box<Ending>),
    /// Nothing more.
    Done,
}

/// What one step of a turn yields: a chunk and where the turn goes after
/// it, or where it goes with nothing to send yet.
type Step = ControlFlow<(Result<Chunk>, Pass), Pass>;

impl Pass {
    /// The next chunk, or the failure that ends the turn.
    async fn next(self, turns: &Turns, request: Request<'_>) -> Option<(Result<Chunk>, Self)> {
        let mut pass = self;
        loop {
            let step = match pass {
                Self::Opening(attempt) => match turns.open(&request).await {
                    Ok(stream) => Step::Continue(Self::Reading(Box::new(Reading {
                        attempt,
                        stream,
                        shown: false,
                        reopen: !request.single_attempt,
                    }))),
                    Err(failure) => Step::Break((Err(failure), Self::Done)),
                },
                Self::Reading(reading) => reading.step(turns).await,
                Self::Ending(mut rest) => match rest.next() {
                    Some(chunk) => Step::Break((chunk, Self::Ending(rest))),
                    None => return None,
                },
                Self::Done => return None,
            };
            match step {
                Step::Break(sent) => return Some(sent),
                Step::Continue(next) => pass = next,
            }
        }
    }
}

/// A turn's stream being read, on its attempt, and whether anything visible
/// went out yet.
struct Reading {
    attempt: usize,
    stream: StreamTurn,
    shown: bool,
    reopen: bool,
}

impl Reading {
    /// Reads the stream's next item.
    async fn step(mut self: Box<Self>, turns: &Turns) -> Step {
        match self.stream.next().await {
            Some(Ok(event)) => match chunk(event) {
                Some(chunk) => {
                    self.shown = true;
                    Step::Break((Ok(chunk), Pass::Reading(self)))
                }
                None => Step::Continue(Pass::Reading(self)),
            },
            Some(Err(_)) | None => self.finished(turns).await,
        }
    }

    /// Known spend leaves before the failure, and prevents reopening a paid attempt.
    async fn finished(self: Box<Self>, turns: &Turns) -> Step {
        let result = self.stream.finish().await;
        let reported = &result.response.usage;
        let known = reported.is_reported();
        match result.failure {
            None => Step::Continue(Pass::Ending(Box::new(ending(
                turns.settings.api,
                result.response,
            )))),
            Some(failure)
                if self.reopen
                    && !known
                    && !self.shown
                    && self.attempt < ATTEMPTS
                    && reopens(&failure) =>
            {
                turns.reopened(self.attempt, &failure);
                Step::Continue(Pass::Opening(self.attempt + 1))
            }
            Some(failure) => {
                let spent = usage::spent(turns.settings.api, reported, &result.response.raw);
                let failure = Err(turns.failed(*failure));
                if known {
                    Step::Continue(Pass::Ending(Box::new(
                        [Ok(Chunk::Usage(spent)), failure].into_iter(),
                    )))
                } else {
                    Step::Break((failure, Pass::Done))
                }
            }
        }
    }
}

/// The chunk one event is, when it is one the seam carries.
fn chunk(event: StreamEvent) -> Option<Chunk> {
    match event {
        StreamEvent::Text { text, .. } => Some(Chunk::Text {
            kind: StreamTextKind::Answer,
            text,
        }),
        StreamEvent::Reasoning { text, .. } => Some(Chunk::Text {
            kind: StreamTextKind::Reasoning,
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
fn ending(api: InferenceApi, response: CompletionResponse) -> Ending {
    let cut = !matches!(
        response.finish_reason(),
        Some(FinishReason::Stop | FinishReason::ToolCalls)
    );
    let spent = usage::spent(api, &response.usage, &response.raw);
    let replay = Replay(response.head().with_content(response.choice));
    let end = End { replay, cut };
    [Ok(Chunk::Usage(spent)), Ok(Chunk::End(end))].into_iter()
}
