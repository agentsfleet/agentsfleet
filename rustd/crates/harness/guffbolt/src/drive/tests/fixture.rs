//! The scripted conversation the loop's cases drive: a model that plays its
//! steps in order, and input that arrives on cue.

use std::collections::{HashMap, VecDeque};

use crate::conversation::{Conversation, Prepared, Sampled};
use crate::turn::{Ending, ModelTurn};

/// What joins the input that arrived between two model calls.
const INPUT_JOIN: &str = "\n\n";

/// One line of the scripted conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Message {
    User(String),
    Assistant(String),
    ToolResult(String),
}

/// One scripted model turn: its answer text and its calls' ids.
#[derive(Debug, Default)]
pub(super) struct Said {
    pub(super) text: String,
    pub(super) calls: Vec<String>,
}

impl ModelTurn for Said {
    fn text(&self) -> &str {
        &self.text
    }

    fn has_calls(&self) -> bool {
        !self.calls.is_empty()
    }

    fn cut(&self) -> bool {
        false
    }
}

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
pub(super) enum Step {
    /// Answers `text` and calls `calls` tools.
    Turn { text: &'static str, calls: usize },
    /// Fails.
    Fail,
    /// The loop is stopped during the call.
    Stop,
    /// Never answers.
    Hang,
}

pub(super) fn answer(text: &'static str) -> Step {
    Step::Turn { text, calls: 0 }
}

pub(super) fn calling(calls: usize) -> Step {
    Step::Turn { text: "", calls }
}

/// A conversation whose model plays `steps` and whose input arrives on cue.
#[derive(Default)]
pub(super) struct Scripted {
    steps: VecDeque<Step>,
    /// Input that arrives while model call N is in flight.
    pub(super) during: HashMap<u64, Vec<&'static str>>,
    /// Input that arrives while the calls of the group started by call N run.
    pub(super) during_calls: HashMap<u64, Vec<&'static str>>,
    /// Stop after this many tool calls of the next group.
    pub(super) stop_after_calls: Option<usize>,
    /// Fitting the conversation to the window fails.
    pub(super) window_fails: bool,
    /// Input that has arrived and is not yet in the conversation.
    pub(super) pending: Vec<&'static str>,
    /// Every input that arrived, in order.
    pub(super) arrived: Vec<&'static str>,
    pub(super) messages: Vec<Message>,
    /// The messages each model call was sent.
    pub(super) sent: Vec<Vec<Message>>,
    stopped: bool,
    sampling: u64,
}

impl Scripted {
    pub(super) fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            messages: vec![Message::User("deploy the fix".to_owned())],
            ..Self::default()
        }
    }

    /// Joins the input that arrived onto the waiting user message, or adds
    /// one; answers whether any came.
    fn take(&mut self) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        let text = std::mem::take(&mut self.pending).join(INPUT_JOIN);
        if let Some(Message::User(waiting)) = self.messages.last_mut() {
            waiting.push_str(INPUT_JOIN);
            waiting.push_str(&text);
        } else {
            self.messages.push(Message::User(text));
        }
        true
    }

    /// Runs `turn`'s calls until the script stops the loop, with the input
    /// due during them arriving first.
    fn run(&mut self, turn: &Said) -> Vec<Message> {
        let arriving = self.during_calls.remove(&self.sampling).unwrap_or_default();
        self.arrived.extend(&arriving);
        self.pending.extend(arriving);
        let mut results = Vec::new();
        for (ran, call) in turn.calls.iter().enumerate() {
            if self.stop_after_calls == Some(ran) {
                self.stopped = true;
                break;
            }
            results.push(Message::ToolResult(call.clone()));
        }
        results
    }

    /// Whether call `index` (from 0) was sent `text` in a user message.
    pub(super) fn call_read(&self, index: usize, text: &str) -> bool {
        self.sent.get(index).is_some_and(|messages| {
            messages
                .iter()
                .any(|message| matches!(message, Message::User(said) if said.contains(text)))
        })
    }

    /// How many tool results the conversation holds.
    pub(super) fn results(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| matches!(message, Message::ToolResult(_)))
            .count()
    }
}

impl Conversation for Scripted {
    type Turn = Said;
    type Results = Vec<Message>;
    type Failure = Failure;

    fn take_input(&mut self) -> impl Future<Output = bool> + Send {
        std::future::ready(self.take())
    }

    fn prepare(&mut self) -> impl Future<Output = Prepared<Failure>> + Send {
        std::future::ready(if self.stopped {
            Prepared::Stopped
        } else if self.window_fails {
            Prepared::Failed(Failure::Window)
        } else {
            Prepared::Ready
        })
    }

    fn begin_group(&mut self) {}

    async fn sample(&mut self, number: u64) -> Sampled<Said, Failure> {
        self.sampling = number;
        self.sent.push(self.messages.clone());
        // Input sent while the model streams arrives now: this call has its
        // request already, so only the next one can read it.
        let arriving = self.during.remove(&number).unwrap_or_default();
        self.arrived.extend(&arriving);
        self.pending.extend(arriving);
        match self.steps.pop_front() {
            Some(Step::Turn { text, calls }) => Sampled::Turn(Said {
                text: text.to_owned(),
                calls: (0..calls)
                    .map(|index| format!("call-{number}-{index}"))
                    .collect(),
            }),
            Some(Step::Fail) | None => Sampled::Failed(Failure::Model),
            Some(Step::Stop) => {
                self.stopped = true;
                Sampled::Stopped
            }
            Some(Step::Hang) => std::future::pending().await,
        }
    }

    fn run_calls(&mut self, turn: &Said) -> impl Future<Output = Vec<Message>> + Send {
        std::future::ready(self.run(turn))
    }

    fn remember(&mut self, turn: Said, results: Vec<Message>) {
        self.messages.push(Message::Assistant(turn.text));
        self.messages.extend(results);
    }

    fn push_user(&mut self, text: String) {
        self.messages.push(Message::User(text));
    }

    fn stopped(&self) -> bool {
        self.stopped
    }

    fn silent(&self, _cut: bool) -> Failure {
        Failure::Silent
    }
}

/// The text of an answered ending.
pub(super) fn answered(ending: &Ending<Failure>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped => None,
    }
}
