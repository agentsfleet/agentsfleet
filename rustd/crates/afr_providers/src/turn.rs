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
//! A call whose arguments are not JSON ends rig's stream. It goes out with the
//! text the model wrote, so the tool refuses it with a reason the model reads
//! and the run goes on, and the turn ends on what arrived before it.

use std::fmt;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};

use afd_wire::activity::StreamTextKind;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use rig_core::DynModel;
use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::error::ProviderError;
use rig_core::message::{AssistantContent, CallId};
use rig_core::operation::Completion;
use rig_core::streaming::{CompletionStream, Item, StreamEvent};

use crate::error::{Error, Result, raise};
use crate::provider::{Call, Chunk, End, Provider, Replay, Request, Usage};
use crate::registry::Wire;
use crate::request;
use crate::retry::ATTEMPTS;

/// The log line a turn opened again writes.
const EVENT_REOPENED: &str = "provider_turn_reopened";
/// The prefix a call the provider sent without an id is given, numbered.
const UNNAMED_CALL: &str = "call_";

/// One lease's provider: a rig model and the wire it speaks.
pub(crate) struct Turns {
    model: DynModel<Completion>,
    wire: Wire,
    lease_id: Box<str>,
    /// The provider as the policy spells it, named when its endpoint is refused.
    provider: Box<str>,
    unnamed: AtomicU64,
}

impl fmt::Debug for Turns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Turns")
            .field("wire", &self.wire)
            .field("lease_id", &self.lease_id)
            .finish_non_exhaustive()
    }
}

impl Turns {
    /// Lease `lease_id`'s provider, `provider` as the policy spells it,
    /// speaking `wire` through `model`.
    pub(crate) fn new(
        model: DynModel<Completion>,
        wire: Wire,
        lease_id: &str,
        provider: &str,
    ) -> Self {
        Self {
            model,
            wire,
            lease_id: lease_id.into(),
            provider: provider.into(),
            unnamed: AtomicU64::new(0),
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

    /// A call to `name` with `arguments`, under a stable id when `id` is
    /// blank.
    fn call(&self, id: &CallId, name: String, arguments: serde_json::Value) -> Call {
        let wire_id = id.wire();
        let id = if wire_id.is_empty() {
            let number = self.unnamed.fetch_add(1, Ordering::Relaxed) + 1;
            format!("{UNNAMED_CALL}{number}")
        } else {
            wire_id.into_owned()
        };
        Call {
            id,
            name,
            arguments,
        }
    }

    /// Opens the turn `request` asks for.
    ///
    /// # Errors
    /// The conversation cannot be sent, or rig could not start the reply.
    fn open(&self, request: &Request<'_>) -> Result<CompletionStream> {
        let built = request::request(self.wire, request)?;
        self.model
            .stream(built)
            .map_err(|failure| self.failed(failure))
    }

    /// Logs a turn opened again after `failure`.
    fn reopened(&self, attempt: usize, failure: &ProviderError) {
        let lease_id = &*self.lease_id;
        let reason = raise::code(failure);
        let event = EVENT_REOPENED;
        tracing::warn!(lease_id, attempt, reason, event);
    }
}

impl Provider for Turns {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>> {
        stream::unfold(Pass::Opening(1), move |pass| pass.next(self, request)).boxed()
    }

    fn accepts_images(&self) -> bool {
        self.wire.carries_images()
    }
}

/// The chunks a turn that ended still has to send: what it spent, then its
/// end.
type Ending = std::array::IntoIter<Chunk, 2>;

/// Where a turn's stream is.
enum Pass {
    /// About to open the stream, on this attempt. The request is built each
    /// opening from the borrowed conversation, so the common turn, opened
    /// once, holds no second copy of it for a reopening that never comes.
    Opening(usize),
    /// Reading it.
    Reading(Reading),
    /// What it spent and how it ended, still to go out.
    Ending(Ending),
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
                Self::Opening(attempt) => match turns.open(&request) {
                    Ok(stream) => Step::Continue(Self::Reading(Reading {
                        attempt,
                        stream,
                        shown: false,
                    })),
                    Err(failure) => Step::Break((Err(failure), Self::Done)),
                },
                Self::Reading(reading) => reading.step(turns).await,
                Self::Ending(mut rest) => match rest.next() {
                    Some(chunk) => Step::Break((Ok(chunk), Self::Ending(rest))),
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
    stream: CompletionStream,
    shown: bool,
}

impl Reading {
    /// Reads the stream's next item.
    async fn step(mut self, turns: &Turns) -> Step {
        match self.stream.next().await {
            Some(Ok(Item::Event(event))) => match chunk(turns, event) {
                Some(chunk) => {
                    self.shown = true;
                    Step::Break((Ok(chunk), Pass::Reading(self)))
                }
                None => Step::Continue(Pass::Reading(self)),
            },
            Some(Ok(Item::Unknown(_))) => Step::Continue(Pass::Reading(self)),
            Some(Err(ProviderError::MalformedToolInput(input))) => {
                let raw = serde_json::Value::String(input.raw);
                let call = turns.call(&input.id, input.name, raw);
                let rest = ending(self.stream.partial());
                Step::Break((Ok(Chunk::Call(call)), Pass::Ending(rest)))
            }
            Some(Err(failure)) if !self.shown && self.attempt < ATTEMPTS && reopens(&failure) => {
                turns.reopened(self.attempt, &failure);
                Step::Continue(Pass::Opening(self.attempt + 1))
            }
            Some(Err(failure)) => Step::Break((Err(turns.failed(failure)), Pass::Done)),
            None => match self.stream.finish().await {
                Ok(response) => Step::Continue(Pass::Ending(ending(response))),
                Err(failure) => Step::Break((Err(turns.failed(failure)), Pass::Done)),
            },
        }
    }
}

/// The chunk one event is, when it is one the seam carries.
fn chunk(turns: &Turns, event: StreamEvent) -> Option<Chunk> {
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
            Some(Chunk::Call(turns.call(
                &call.id,
                name,
                call.function.arguments,
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
    failure.provider_response_status().is_none()
        && failure.is_retryable()
        && !raise::oversize(failure)
        && !raise::blocked(failure)
}

/// The chunks a finished turn ends on: what it spent, then its end.
fn ending(response: CompletionResponse) -> Ending {
    let usage = response.usage;
    // rig counts cache reads inside `input_tokens` on every wire; the daemon
    // bills `input` and `cached_input` as two disjoint counts, so the reads
    // come out of the prompt total here, once, where every wire meets.
    let cached_input = usage.cached_input_tokens.unwrap_or(0);
    let spent = Usage {
        input: usage.input_tokens.unwrap_or(0).saturating_sub(cached_input),
        cached_input,
        output: usage.output_tokens.unwrap_or(0),
    };
    let cut = response.finish_reason() == Some(FinishReason::Length);
    let reasoning = response
        .choice
        .into_iter()
        .filter(|content| matches!(content, AssistantContent::Reasoning(_)));
    let end = End {
        replay: Replay(reasoning.collect()),
        cut,
    };
    [Chunk::Usage(spent), Chunk::End(end)].into_iter()
}
