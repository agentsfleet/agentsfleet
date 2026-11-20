//! A run whose root and children share one scripted model and the tools
//! `Tools::builder()` built, the way an embedder wires subagents: the root
//! and every child dispatch through the same tools, and `Raise` hands the
//! harness each child's conversation.

use std::future::ready;

use guffbolt::agents::{Agents, Children, Raise, Requests, Seat};
use guffbolt::tools::{Context, ContextTool};
use guffbolt::{
    Call, CallRef, Conversation, DriveState, Ending, Extension, Input, Message, Request,
    ToolFuture, ToolSpec, Tools, Transcript, Unobserved, Window, drive,
};
use tokio_util::sync::CancellationToken;

use crate::script::{Failure, Script};

/// The key the root's requests carry.
pub(crate) const ROOT: &str = "root";

/// The key child `id`'s requests carry.
pub(crate) fn agent(id: u64) -> String {
    format!("agent-{id}")
}

/// What every loop of the run shares: the children, the model and the tools.
pub(crate) struct Family<'t> {
    pub(crate) agents: Agents,
    pub(crate) script: Script,
    pub(crate) tools: &'t Tools<ContextTool<'t>>,
}

impl<'t> Family<'t> {
    /// The root's conversation, opening with `opening`.
    pub(crate) fn root(&self, opening: Vec<Message>) -> Chat<'_, 't> {
        Chat {
            family: self,
            key: ROOT.to_owned(),
            transcript: Transcript::new(opening),
            stop: CancellationToken::new(),
        }
    }

    /// Drives the root from `state` over `opening` with `extension`, its
    /// children beside it, and ends the children once it ends; answers its
    /// ending and its transcript.
    pub(crate) async fn run<E>(
        &self,
        requests: Requests,
        opening: Vec<Message>,
        extension: &mut E,
        state: &mut DriveState,
    ) -> (Ending<Failure>, Vec<Message>)
    where
        for<'f> E: Extension<Chat<'f, 't>>,
    {
        let mut root = self.root(opening);
        let mut children = Children::new(requests);
        let ending = children
            .beside(self, drive(&mut root, extension, state))
            .await;
        let _quiet = children.end_all(&self.agents);
        (ending, root.transcript.messages().to_vec())
    }
}

/// One loop of the run: the root, or one of a child's turns.
pub(crate) struct Chat<'f, 't> {
    family: &'f Family<'t>,
    key: String,
    pub(crate) transcript: Transcript,
    stop: CancellationToken,
}

impl Conversation for Chat<'_, '_> {
    type Failure = Failure;
    type Model = Script;
    type Observer = Unobserved;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(Vec::new())
    }

    fn model(&self) -> &Script {
        &self.family.script
    }

    fn observer(&self) -> &Unobserved {
        &Unobserved
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.family.tools.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "script",
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

    fn silent(&self, cut: bool) -> Failure {
        Failure::Silent(cut)
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

pub(crate) fn user(text: &str) -> Message {
    Message::User(text.to_owned())
}

pub(crate) fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: guffbolt::Replay::default(),
    }
}

/// The text of an answered ending.
pub(crate) fn answered(ending: &Ending<Failure>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

/// The value at `pointer` in `value`, or null when there is none.
pub(crate) fn at<'v>(value: &'v serde_json::Value, pointer: &str) -> &'v serde_json::Value {
    value.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}
