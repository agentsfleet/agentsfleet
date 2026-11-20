//! A scripted model: each loop plays the steps written for it, keyed by the
//! cache key its requests carry, and every request is kept for the test to
//! read back.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use guffbolt::{Call, Chunk, End, Message, Model, Request, TextKind, Usage};
use serde_json::Value;

/// What one model call of one loop does.
#[derive(Debug)]
pub(crate) enum Play {
    /// Answers with this text.
    Say(&'static str),
    /// Calls each tool, with its arguments, in order.
    Call(Vec<(&'static str, Value)>),
    /// Calls each tool, then stops at the output limit.
    Cut(Vec<(&'static str, Value)>),
    /// Never answers.
    Hang,
}

/// Calls `name` with `arguments`, alone in its step.
pub(crate) fn call(name: &'static str, arguments: Value) -> Play {
    Play::Call(vec![(name, arguments)])
}

/// Why a scripted loop failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Its loop's script ran out.
    Exhausted,
    /// The model answered nothing; `true` when its step was cut.
    Silent(bool),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => f.write_str("the script ran out"),
            Self::Silent(cut) => write!(f, "the model answered nothing (cut: {cut})"),
        }
    }
}

/// One request a loop sent: its key, its messages and the tools it offered.
#[derive(Debug, Clone)]
pub(crate) struct Sent {
    pub(crate) key: String,
    pub(crate) messages: Vec<Message>,
    pub(crate) tools: Vec<String>,
}

/// A model that plays each loop's script and keeps every request.
#[derive(Debug, Default)]
pub(crate) struct Script {
    plays: Mutex<HashMap<String, VecDeque<Play>>>,
    sent: Mutex<Vec<Sent>>,
}

impl Script {
    /// The loop keyed `key` plays `plays`, in order.
    #[must_use]
    pub(crate) fn plays(
        self,
        key: impl Into<String>,
        plays: impl IntoIterator<Item = Play>,
    ) -> Self {
        lock(&self.plays).insert(key.into(), plays.into_iter().collect());
        self
    }

    /// Every request the loop keyed `key` sent, in order.
    pub(crate) fn sent_by(&self, key: &str) -> Vec<Sent> {
        lock(&self.sent)
            .iter()
            .filter(|sent| sent.key == key)
            .cloned()
            .collect()
    }
}

impl Model for Script {
    type Error = Failure;

    fn measure(&self, _request: Request<'_>) -> Result<u64, Failure> {
        Ok(0)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Failure>> {
        let key = request.cache_key.to_owned();
        let number = {
            let mut sent = lock(&self.sent);
            sent.push(Sent {
                key: key.clone(),
                messages: request.messages.to_vec(),
                tools: request
                    .tools
                    .iter()
                    .map(|tool| tool.name.to_owned())
                    .collect(),
            });
            sent.iter().filter(|sent| sent.key == key).count()
        };
        let play = lock(&self.plays)
            .get_mut(&key)
            .and_then(VecDeque::pop_front);
        match play {
            Some(Play::Say(text)) => {
                let said = Chunk::Text {
                    kind: TextKind::Answer,
                    text: text.to_owned(),
                };
                // What the step spent rides before its end, as a provider
                // reports it.
                let spent = Chunk::Usage(Usage::default());
                stream::iter([Ok(said), Ok(spent), Ok(Chunk::End(End::default()))]).boxed()
            }
            Some(Play::Call(calls)) => calls_then(&key, number, calls, false),
            Some(Play::Cut(calls)) => calls_then(&key, number, calls, true),
            Some(Play::Hang) => stream::pending().boxed(),
            None => stream::iter([Err(Failure::Exhausted)]).boxed(),
        }
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// `calls`, each id naming its loop, step and place, then the step's end.
fn calls_then<'a>(
    key: &str,
    number: usize,
    calls: Vec<(&'static str, Value)>,
    cut: bool,
) -> BoxStream<'a, Result<Chunk, Failure>> {
    let mut chunks: Vec<Result<Chunk, Failure>> = calls
        .into_iter()
        .enumerate()
        .map(|(index, (name, arguments))| {
            Ok(Chunk::Call(Call {
                id: format!("{key}-{number}-{index}"),
                name: name.to_owned(),
                arguments,
            }))
        })
        .collect();
    chunks.push(Ok(Chunk::End(End {
        cut,
        ..End::default()
    })));
    stream::iter(chunks).boxed()
}

/// Every tool result `messages` holds, in order.
pub(crate) fn results(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.clone()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect()
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
