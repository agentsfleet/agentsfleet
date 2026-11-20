//! A run whose root and children share one scripted model and the seven
//! subagent tools, built through the builder: each loop plays the steps
//! scripted for it, keyed by the cache key its requests carry.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::ready;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agents::{Agents, Raise, Seat};
use crate::conversation::Conversation;
use crate::drive::{DriveState, Input};
use crate::extension::Extension;
use crate::model::{Chunk, End, Model, Request, TextKind};
use crate::observe::Unobserved;
use crate::step::Ending;
use crate::tool::{ToolFuture, ToolSpec, Tools};
use crate::tools::{Context, ContextTool, ToolsConfig};
use crate::transcript::{Call, CallRef, Message, Transcript};
use crate::window::Window;

/// The key the root's requests carry.
pub(super) const ROOT: &str = "root";

/// The key child `id`'s requests carry.
pub(super) fn agent(id: u64) -> String {
    format!("agent-{id}")
}

/// What one model call of one loop does.
#[derive(Debug)]
pub(super) enum Play {
    /// Answers with this text.
    Say(&'static str),
    /// Calls each tool, with its arguments, in order.
    Call(Vec<(&'static str, Value)>),
    /// Never answers.
    Hang,
}

/// Calls `name` with `arguments`, alone in its step.
pub(super) fn call(name: &'static str, arguments: Value) -> Play {
    Play::Call(vec![(name, arguments)])
}

/// Why a scripted call failed: its loop's script ran out.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Exhausted;

impl fmt::Display for Exhausted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the script ran out")
    }
}

/// One request a loop sent: its key, its messages and the tools it offered.
#[derive(Debug, Clone)]
pub(super) struct Sent {
    pub(super) key: String,
    pub(super) messages: Vec<Message>,
    pub(super) tools: Vec<String>,
}

/// A model that plays each loop's script and keeps every request.
#[derive(Debug, Default)]
pub(super) struct Puppet {
    plays: Mutex<HashMap<String, VecDeque<Play>>>,
    sent: Mutex<Vec<Sent>>,
}

impl Puppet {
    /// The loop keyed `key` plays `plays`, in order.
    pub(super) fn plays(
        self,
        key: impl Into<String>,
        plays: impl IntoIterator<Item = Play>,
    ) -> Self {
        lock(&self.plays).insert(key.into(), plays.into_iter().collect());
        self
    }

    /// Every request the loop keyed `key` sent, in order.
    pub(super) fn sent_by(&self, key: &str) -> Vec<Sent> {
        lock(&self.sent)
            .iter()
            .filter(|sent| sent.key == key)
            .cloned()
            .collect()
    }
}

impl Model for Puppet {
    type Error = Exhausted;

    fn measure(&self, _request: Request<'_>) -> Result<u64, Exhausted> {
        Ok(0)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Exhausted>> {
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
        let end = Chunk::End(End::default());
        match play {
            Some(Play::Say(text)) => {
                let said = Chunk::Text {
                    kind: TextKind::Answer,
                    text: text.to_owned(),
                };
                stream::iter([Ok(said), Ok(end)]).boxed()
            }
            Some(Play::Call(calls)) => {
                let calls = calls
                    .into_iter()
                    .enumerate()
                    .map(move |(index, (name, arguments))| {
                        Chunk::Call(Call {
                            id: format!("{key}-{number}-{index}"),
                            name: name.to_owned(),
                            arguments,
                        })
                    });
                stream::iter(calls.chain([end]).map(Ok)).boxed()
            }
            Some(Play::Hang) => stream::pending().boxed(),
            None => stream::iter([Err(Exhausted)]).boxed(),
        }
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// What every loop of the run shares: the children, the model and the
/// tools, which the root and every child dispatch through.
pub(super) struct Family<'t> {
    pub(super) agents: Agents,
    pub(super) puppet: Puppet,
    pub(super) tools: &'t Tools<ContextTool<'t>>,
}

impl<'t> Family<'t> {
    /// The root's conversation, opening with `opening`.
    pub(super) fn root(&self, opening: Vec<Message>) -> Chat<'_, 't> {
        Chat {
            family: self,
            key: ROOT.to_owned(),
            transcript: Transcript::new(opening),
            stop: CancellationToken::new(),
        }
    }
}

/// The tools `agents` builds: the seven, and nothing else.
pub(super) fn tools(agents: &Agents) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(["*"]).expect("one pattern");
    Tools::builder()
        .with_agents(agents)
        .build(&config)
        .expect("the run's children are given")
}

/// One loop of the run: the root, or a child's turn.
pub(super) struct Chat<'f, 't> {
    family: &'f Family<'t>,
    key: String,
    pub(super) transcript: Transcript,
    stop: CancellationToken,
}

impl Conversation for Chat<'_, '_> {
    type Failure = Exhausted;
    type Model = Puppet;
    type Observer = Unobserved;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(Vec::new())
    }

    fn model(&self) -> &Puppet {
        &self.family.puppet
    }

    fn observer(&self) -> &Unobserved {
        &Unobserved
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.family.tools.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "puppet",
            instructions: "",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: &self.key,
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, call_ref: &'a CallRef) -> ToolFuture<'a> {
        let context = Context {
            call_ref,
            run_id: "run-1",
            wake: None,
        };
        self.family
            .tools
            .dispatch(&call.name, &call.arguments, context)
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> Exhausted {
        Exhausted
    }
}

impl<'t> Raise for Family<'t> {
    type Child<'s>
        = Chat<'s, 't>
    where
        Self: 's;

    fn agents(&self) -> &Agents {
        &self.agents
    }

    fn child(&self, seat: Seat) -> Chat<'_, 't> {
        Chat {
            family: self,
            key: agent(seat.id),
            transcript: seat.transcript,
            stop: seat.stop,
        }
    }
}

/// The root's extension that saves the run's children into a copy of its
/// state before model call `at`, under a pause, as an embedder's checkpoint
/// does.
pub(super) struct Saving<'a> {
    pub(super) agents: &'a Agents,
    pub(super) at: u64,
    pub(super) saved: Option<DriveState>,
}

impl<'f, 't> Extension<Chat<'f, 't>> for Saving<'_> {
    async fn before_model(
        &mut self,
        _root: &mut Chat<'f, 't>,
        number: u64,
        state: &DriveState,
    ) -> Option<Ending<Exhausted>> {
        if number != self.at {
            return None;
        }
        let pause = self.agents.pause().await?;
        let mut saved = state.clone();
        if self.agents.save(&pause, &mut saved) {
            self.saved = Some(saved);
        }
        None
    }
}

/// Every tool result `messages` holds, in order.
pub(super) fn results(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.clone()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect()
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
