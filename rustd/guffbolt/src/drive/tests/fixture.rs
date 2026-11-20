//! The scripted conversation the loop's cases drive: a model that plays its
//! steps in order (`script.rs`), input that arrives on cue, and the tools its
//! calls reach by name. What it hears of the loop is in `heard.rs`.

use std::collections::HashMap;
use std::future::{pending, ready};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::script::{ASK_USER, Cue, Failure, Script, TOOL, lock, user};
use crate::conversation::Conversation;
use crate::drive::{Input, Mark};
use crate::model::Request;
use crate::step::Ending;
use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};
use crate::transcript::{Call, CallRef, Message, Transcript};
use crate::window::{Fitted, Window};

/// The output limit a fitted window sets.
pub(super) const FITTED_LIMIT: u32 = 64;

/// What the scripted conversation lends each call: where the call sits.
pub(super) type Lent<'c> = &'c CallRef;

/// A scripted tool, as the conversation holds it.
pub(super) type Held = dyn for<'c> Tool<Lent<'c>>;

/// The tools a scripted run holds: `deploy`, which answers its call's id,
/// and `ask_user`, which asks the person the question it is given; a case
/// offers more ([`Scripted::offering`]).
pub(super) struct Kit(Tools<Held>);

impl Default for Kit {
    fn default() -> Self {
        let parameters = json!({ "type": "object" });
        Self(Tools::new(vec![
            Box::new(Deploy(parameters.clone())),
            Box::new(Ask(parameters)),
        ]))
    }
}

/// `deploy`: answers with the id of the call that ran it.
#[derive(Debug)]
struct Deploy(Value);

impl Describe for Deploy {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: TOOL,
            description: "Deploys the fix.",
            parameters: &self.0,
        }
    }
}

impl<'c> Tool<Lent<'c>> for Deploy {
    fn call<'a>(&'a self, _arguments: &'a Value, cx: Lent<'c>) -> ToolFuture<'a>
    where
        Lent<'c>: 'a,
    {
        Box::pin(ready(ToolOutput::succeeded(cx.call_id.as_str())))
    }
}

/// `ask_user`: asks the person the question its arguments carry.
#[derive(Debug)]
struct Ask(Value);

impl Describe for Ask {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: ASK_USER,
            description: "Asks the person a question.",
            parameters: &self.0,
        }
    }
}

impl<'c> Tool<Lent<'c>> for Ask {
    fn call<'a>(&'a self, arguments: &'a Value, _cx: Lent<'c>) -> ToolFuture<'a>
    where
        Lent<'c>: 'a,
    {
        let question = arguments.get("question").and_then(Value::as_str);
        Box::pin(ready(ToolOutput::asks(question.unwrap_or_default())))
    }
}

/// A conversation whose model plays its script and whose input arrives on
/// cue.
#[derive(Default)]
pub(super) struct Scripted {
    model: Script,
    /// Input that arrives while model call N is in flight.
    pub(super) during: HashMap<u64, Vec<&'static str>>,
    /// Input that arrives while the calls of the group started by call N run.
    pub(super) during_calls: HashMap<u64, Vec<&'static str>>,
    /// Stop as the call after this many tool calls of the next group starts.
    pub(super) stop_after_calls: Option<usize>,
    /// Fitting the conversation to the window fails.
    pub(super) window_fails: bool,
    /// Each call's window keeps only the last this many messages.
    pub(super) fitted: Option<usize>,
    /// Every tool call runs until something drops it.
    pub(super) calls_hang: bool,
    /// What the observer leaves of each call's output for the model.
    pub(super) mask: Option<&'static str>,
    /// The tools each call reaches by name.
    tools: Kit,
    pub(super) transcript: Transcript,
    /// Input that has arrived and is not yet in the conversation.
    pending: Mutex<Vec<&'static str>>,
    /// Every input that arrived, in order.
    arrived: Mutex<Vec<&'static str>>,
    /// The loop's stop, shared with the script, whose steps may pull it.
    pub(super) stop: CancellationToken,
    /// The model call in flight, or the one whose calls run.
    pub(super) sampling: AtomicU64,
    /// How many calls of the current group started.
    pub(super) ran: AtomicUsize,
    /// What the observer heard, in order.
    pub(super) heard: Mutex<Vec<String>>,
    /// Where each call the loop dispatched sits, in order.
    dispatched: Mutex<Vec<CallRef>>,
    /// The mark each input arrives with, by its text; input not named here
    /// arrives unmarked.
    pub(super) marks: HashMap<&'static str, &'static str>,
    /// Each call's step and the mark its end carried, in order.
    pub(super) ended_marks: Mutex<Vec<(u64, Option<String>)>>,
}

impl Scripted {
    pub(super) fn new(steps: impl IntoIterator<Item = Cue>) -> Self {
        let stop = CancellationToken::new();
        Self {
            model: Script {
                steps: Mutex::new(steps.into_iter().collect()),
                stop: stop.clone(),
                ..Script::default()
            },
            transcript: Transcript::new(vec![user("deploy the fix")]),
            stop,
            ..Self::default()
        }
    }

    /// The same run, offering `tool` after its own.
    pub(super) fn offering(mut self, tool: Box<Held>) -> Self {
        self.tools.0.offer(tool);
        self
    }

    /// Where each call the loop dispatched sits, in order.
    pub(super) fn dispatched(&self) -> Vec<CallRef> {
        lock(&self.dispatched).clone()
    }

    /// `text` arrives now, before the next model call.
    pub(super) fn sends(&self, text: &'static str) {
        lock(&self.arrived).push(text);
        lock(&self.pending).push(text);
    }

    /// Each call's step and the mark its end carried, in order.
    pub(super) fn ended_marks(&self) -> Vec<(u64, Option<String>)> {
        lock(&self.ended_marks).clone()
    }

    /// The input `cues` hold for `number`, when they hold any, arrives now.
    pub(super) fn arrive(&self, cues: &HashMap<u64, Vec<&'static str>>, number: u64) {
        let arriving = cues.get(&number).map_or(&[][..], Vec::as_slice);
        lock(&self.arrived).extend(arriving);
        lock(&self.pending).extend(arriving);
    }

    /// The messages each model call was sent.
    pub(super) fn sent(&self) -> Vec<Vec<Message>> {
        lock(&self.model.sent).clone()
    }

    /// The output limit each model call was sent.
    pub(super) fn limits(&self) -> Vec<Option<u32>> {
        lock(&self.model.limits).clone()
    }

    /// The conversation so far.
    pub(super) fn messages(&self) -> &[Message] {
        self.transcript.messages()
    }

    /// Input that has arrived and is not yet in the conversation.
    pub(super) fn pending(&self) -> Vec<&'static str> {
        lock(&self.pending).clone()
    }

    /// Every input that arrived, in order.
    pub(super) fn arrived(&self) -> Vec<&'static str> {
        lock(&self.arrived).clone()
    }

    /// Whether call `index` (from 0) was sent `text` in a user message.
    pub(super) fn call_read(&self, index: usize, text: &str) -> bool {
        self.sent().get(index).is_some_and(|messages| {
            messages
                .iter()
                .any(|message| matches!(message, Message::User(said) if said.contains(text)))
        })
    }

    /// How many tool results the conversation holds.
    pub(super) fn results(&self) -> usize {
        self.messages()
            .iter()
            .filter(|message| matches!(message, Message::ToolResult { .. }))
            .count()
    }

    /// What the observer heard, in order.
    pub(super) fn heard(&self) -> Vec<String> {
        lock(&self.heard).clone()
    }
}

impl Conversation for Scripted {
    type Failure = Failure;
    type Model = Script;
    type Observer = Self;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        let taken = std::mem::take(&mut *lock(&self.pending));
        let marks = &self.marks;
        ready(
            taken
                .into_iter()
                .map(|text| Input {
                    text: text.to_owned(),
                    mark: marks.get(text).copied().map(Mark::new),
                })
                .collect(),
        )
    }

    fn window(&mut self) -> impl Future<Output = Fitted<Failure>> + Send {
        let messages = self.transcript.messages();
        ready(if self.stop.is_cancelled() {
            Fitted::Stopped
        } else if self.window_fails {
            Fitted::Failed(Failure::Window)
        } else if let Some(last) = self.fitted {
            let kept = messages.get(messages.len().saturating_sub(last)..);
            Fitted::Ready(Window {
                messages: kept.unwrap_or_default().to_vec(),
                max_output_tokens: Some(FITTED_LIMIT),
            })
        } else {
            Fitted::Ready(Window::whole(messages))
        })
    }

    fn model(&self) -> &Script {
        &self.model
    }

    fn observer(&self) -> &Self {
        self
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.tools.0.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "scripted",
            instructions: "",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: "",
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, call_ref: &'a CallRef) -> ToolFuture<'a> {
        lock(&self.dispatched).push(call_ref.clone());
        if self.calls_hang {
            return Box::pin(pending());
        }
        self.tools.0.dispatch(&call.name, &call.arguments, call_ref)
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> Failure {
        Failure::Silent
    }
}

/// The text of an answered ending.
pub(super) fn answered(ending: &Ending<Failure>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}
