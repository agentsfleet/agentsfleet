//! The model the loop's cases drive: it plays its steps in order and keeps
//! what each call was sent. The messages its steps say are built here too.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::model::{Chunk, End, Model, Request, TextKind};
use crate::transcript::{Call, Message, Replay};

/// The tool every scripted call names.
const TOOL: &str = "deploy";

/// Why a scripted call failed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// The script failed the model call.
    Model,
    /// The script failed to fit the conversation to the window.
    Window,
    /// The model answered nothing.
    Silent,
}

/// What one model call does.
#[derive(Debug)]
pub(super) enum Cue {
    /// Answers `text` and calls `calls` tools.
    Reply { text: &'static str, calls: usize },
    /// Calls this many tools and stops at the output limit.
    Cut(usize),
    /// Fails.
    Fail,
    /// The loop is stopped during the call.
    Stop,
    /// Never answers.
    Hang,
}

pub(super) fn answer(text: &'static str) -> Cue {
    Cue::Reply { text, calls: 0 }
}

pub(super) fn calling(calls: usize) -> Cue {
    Cue::Reply { text: "", calls }
}

/// The user saying `text`.
pub(super) fn user(text: &str) -> Message {
    Message::User(text.to_owned())
}

/// The model answering `text`, calling nothing.
pub(super) fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

/// Model call `number` saying nothing and making `calls` calls.
pub(super) fn called(number: usize, calls: usize) -> Message {
    Message::Assistant {
        text: String::new(),
        calls: (0..calls).map(|index| call(number, index)).collect(),
        replay: Replay::default(),
    }
}

/// What the call with id `id` answered.
pub(super) fn result(id: &str) -> Message {
    Message::ToolResult {
        call_id: id.to_owned(),
        output: id.to_owned(),
        image: None,
    }
}

/// Call `index` of model call `number`.
fn call(number: usize, index: usize) -> Call {
    Call {
        id: format!("call-{number}-{index}"),
        name: TOOL.to_owned(),
        arguments: Value::Null,
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A model that plays `steps` and keeps the messages and the output limit
/// each call was sent.
#[derive(Debug, Default)]
pub(super) struct Script {
    pub(super) steps: Mutex<VecDeque<Cue>>,
    /// The messages each call was sent.
    pub(super) sent: Mutex<Vec<Vec<Message>>>,
    /// The output limit each call was sent.
    pub(super) limits: Mutex<Vec<Option<u32>>>,
    /// The loop's stop, which a step may pull during its call.
    pub(super) stop: CancellationToken,
}

impl Model for Script {
    type Error = Failure;

    fn measure(&self, request: Request<'_>) -> Result<u64, Failure> {
        Ok(u64::try_from(request.messages.len()).unwrap_or(u64::MAX))
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Failure>> {
        let number = {
            let mut sent = lock(&self.sent);
            sent.push(request.messages.to_vec());
            sent.len()
        };
        lock(&self.limits).push(request.max_output_tokens);
        let step = lock(&self.steps).pop_front();
        match step {
            Some(Cue::Reply { text, calls }) => {
                let said = (!text.is_empty()).then(|| Chunk::Text {
                    kind: TextKind::Answer,
                    text: text.to_owned(),
                });
                let calls = (0..calls).map(move |index| Chunk::Call(call(number, index)));
                let chunks = said
                    .into_iter()
                    .chain(calls)
                    .chain([Chunk::End(End::default())]);
                stream::iter(chunks.map(Ok)).boxed()
            }
            Some(Cue::Cut(calls)) => {
                let calls = (0..calls).map(move |index| Chunk::Call(call(number, index)));
                let cut = End {
                    cut: true,
                    ..End::default()
                };
                stream::iter(calls.chain([Chunk::End(cut)]).map(Ok)).boxed()
            }
            Some(Cue::Fail) | None => stream::iter([Err(Failure::Model)]).boxed(),
            // The stream never ends, so only the stop ends the call.
            Some(Cue::Stop) => {
                self.stop.cancel();
                stream::pending().boxed()
            }
            Some(Cue::Hang) => stream::pending().boxed(),
        }
    }

    fn accepts_images(&self) -> bool {
        false
    }
}
