//! The streamed turn: one opening at a time, the chunks each event becomes,
//! and the rule for opening a turn again. Spend already known and output
//! already shown fence a turn against a second opening.

use std::ops::ControlFlow;

use rig_core::completion::{CompletionResponse, FinishReason};
use rig_core::error::ProviderError;
use rig_core::message::AssistantContent;
use rig_core::streaming::StreamEvent;

use super::super::route::Api;
use super::super::stream::StreamTurn;
use super::super::usage;
use super::Connected;
use crate::{Call, Chunk, End, Error, Replay, Request, Result, TextKind};

/// The chunks a turn that ended still has to send: what it spent, then its
/// end.
type Ending = std::array::IntoIter<Result<Chunk>, 2>;

/// Where a turn's stream is.
pub(super) enum Pass {
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
    pub(super) async fn next(
        self,
        connected: &Connected,
        request: Request<'_>,
    ) -> Option<(Result<Chunk>, Self)> {
        let mut pass = self;
        loop {
            let step = match pass {
                Self::Opening(attempt) => match connected.open(&request).await {
                    Ok(stream) => Step::Continue(Self::Reading(Box::new(Reading {
                        attempt,
                        stream,
                        shown: false,
                        reopen: !request.single_attempt,
                    }))),
                    Err(failure) => Step::Break((Err(failure), Self::Done)),
                },
                Self::Reading(reading) => reading.step(connected).await,
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
pub(super) struct Reading {
    attempt: usize,
    stream: StreamTurn,
    shown: bool,
    reopen: bool,
}

impl Reading {
    /// Reads the stream's next item.
    async fn step(mut self: Box<Self>, connected: &Connected) -> Step {
        match self.stream.next().await {
            Some(Ok(event)) => match chunk(event) {
                Some(chunk) => {
                    self.shown = true;
                    Step::Break((Ok(chunk), Pass::Reading(self)))
                }
                None => Step::Continue(Pass::Reading(self)),
            },
            Some(Err(_)) | None => self.finished(connected).await,
        }
    }

    /// Known spend leaves before the failure, and prevents reopening a paid attempt.
    async fn finished(self: Box<Self>, connected: &Connected) -> Step {
        let api = connected.route.api();
        let result = self.stream.finish().await;
        let reported = &result.response.usage;
        let known = reported.is_reported();
        match result.failure {
            None => Step::Continue(Pass::Ending(Box::new(ending(api, result.response)))),
            Some(failure)
                if self.reopen
                    && !known
                    && !self.shown
                    && self.attempt < connected.retry.openings()
                    && reopens(&failure) =>
            {
                connected.reopened(self.attempt, *failure);
                Step::Continue(Pass::Opening(self.attempt + 1))
            }
            Some(failure) => {
                let spent = usage::spent(api, reported, &result.response.raw);
                let failure = Err(Error::provider(*failure));
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
        } => Some(Chunk::Call(Call {
            // The provider's call identity, or the local one rig issued.
            id: call.id.wire().into_owned(),
            name: String::from(call.function.name),
            arguments: call.function.invalid_arguments.map_or_else(
                || serde_json::Value::Object(call.function.arguments),
                serde_json::Value::String,
            ),
        })),
        StreamEvent::Start { .. } | StreamEvent::Arguments { .. } | StreamEvent::End { .. } => None,
    }
}

/// Whether a failure mid-stream may pass on a second opening: one the
/// provider sent after its reply began, with no status of its own. A reply
/// the transport ended at its bound is not one: the next would be as long.
/// Nor is a destination the embedder's guard refused: it is refused the same
/// way again.
fn reopens(failure: &ProviderError) -> bool {
    failure.provider_response().is_none()
        && failure.is_retryable()
        && !crate::error::oversize(failure)
        && !crate::error::refused(failure)
}

/// The chunks a finished turn on `api` ends on: what it spent, then its end.
/// The end carries the turn's own message as its replay; one that will not
/// encode fails the turn after its spend is sent, never drops the record.
fn ending(api: Api, response: CompletionResponse) -> Ending {
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
        .map_err(Error::unreadable);
    [Ok(Chunk::Usage(spent)), end].into_iter()
}
