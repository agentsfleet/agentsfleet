//! The streamed turn owns one opening and preserves output and spend fences.

use super::{Chunk, Ending, Request, Result, Turns, chunk, ending, reopens, usage};
use af_inference::stream::StreamTurn;
use std::ops::ControlFlow;

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
        turns: &Turns,
        request: Request<'_>,
    ) -> Option<(Result<Chunk>, Self)> {
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
pub(super) struct Reading {
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
                    && self.attempt < turns.retries.openings
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
