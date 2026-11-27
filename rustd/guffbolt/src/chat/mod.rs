//! A ready chat: the loop over any [`Model`], with instructions and,
//! optionally, tools that are lent nothing.
//!
//! A chat that needs more, such as a context its tools share, a fitted
//! window or an observer, writes its own [`Conversation`].

use std::fmt;
use std::future::ready;

use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use tokio_util::sync::CancellationToken;

use crate::drive::{DriveState, Input, drive};
use crate::extension::Plain;
use crate::observe::Unobserved;
use crate::step::Ending;
use crate::tool::{Tool, ToolFuture, Tools};
use crate::transcript::{CallRef, Transcript};
use crate::window::Window;
use guffbolt_ai::{Call, Chunk, Model, Request, ToolSpec};

/// A conversation over model `M`: its instructions, its tools, the
/// transcript it keeps and its stop.
///
/// [`Chat::prompt`] sends one message and drives the loop with [`Plain`] to
/// its end. The transcript carries every prompt, so the next one follows the
/// last answer.
#[derive(Debug)]
pub struct Chat<M> {
    model: Lifted<M>,
    instructions: String,
    /// The tools the model is offered; none until [`Chat::with_tools`].
    tools: Tools<dyn Tool<()>>,
    transcript: Transcript,
    /// What the loop kept of the prompts before, so a call numbers after
    /// them and a question waits for the next prompt.
    state: DriveState,
    /// The prompt not yet in the transcript; the loop takes it at the top of
    /// its first step.
    inbox: Vec<Input>,
    stop: CancellationToken,
}

impl<M> Chat<M>
where
    M: Model + 'static,
    M::Error: Send + 'static,
{
    /// A chat with `model`, which sends `instructions` as the system prompt
    /// of every call and offers no tool.
    #[must_use]
    pub fn new(model: M, instructions: impl Into<String>) -> Self {
        Self {
            model: Lifted(model),
            instructions: instructions.into(),
            tools: Tools::new(Vec::new()),
            transcript: Transcript::default(),
            state: DriveState::default(),
            inbox: Vec::new(),
            stop: CancellationToken::new(),
        }
    }

    /// The same chat, offering `tools`. A tool is lent nothing: what its
    /// calls share, it holds itself.
    #[must_use]
    pub fn with_tools(mut self, tools: Tools<dyn Tool<()>>) -> Self {
        self.tools = tools;
        self
    }

    /// Sends `text` and runs the loop until the model answers, fails, is
    /// stopped or asks the person a question. The ending answers this prompt
    /// alone. A chat waiting on a question reads `text` as its answer.
    pub async fn prompt(&mut self, text: impl Into<String>) -> Ending<ChatFailure<M::Error>> {
        let text = text.into();
        if self.state.waiting().is_some() {
            self.state.resume(&mut self.transcript, text);
        } else {
            self.inbox.push(Input::from(text));
        }
        // The loop borrows the chat and its state apart; a prompt whose
        // future is dropped leaves a fresh state, and the transcript keeps
        // every message the loop remembered.
        let mut state = std::mem::take(&mut self.state);
        state.begin_turn();
        let ending = drive(self, &mut Plain, &mut state).await;
        self.state = state;
        ending
    }

    /// The token that stops this chat: clone it before [`Chat::prompt`] and
    /// cancel it from elsewhere. A cancelled chat stays stopped, so every
    /// later prompt ends [`Ending::Stopped`].
    #[must_use]
    pub const fn stop(&self) -> &CancellationToken {
        &self.stop
    }
}

impl<M> crate::Conversation for Chat<M>
where
    M: Model + 'static,
    M::Error: Send + 'static,
{
    type Failure = ChatFailure<M::Error>;
    // The model as the loop calls it, its failures the chat's. Named as a
    // trait object, so the wrapper stays private.
    type Model = dyn Model<Error = ChatFailure<M::Error>>;
    type Observer = Unobserved;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(std::mem::take(&mut self.inbox))
    }

    fn model(&self) -> &Self::Model {
        &self.model
    }

    fn observer(&self) -> &Unobserved {
        &Unobserved
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.tools.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            // A model names its own id: `Model` carries none to send here.
            model: "",
            instructions: &self.instructions,
            messages: &window.messages,
            tools,
            cache_key: "",
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, _call_ref: &'a CallRef) -> ToolFuture<'a> {
        self.tools.dispatch(&call.name, &call.arguments, ())
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, cut: bool) -> ChatFailure<M::Error> {
        ChatFailure::Silent { cut }
    }
}

/// Why a chat's prompt failed: its model's own error, or a first step that
/// answered nothing.
#[derive(Debug)]
pub enum ChatFailure<E> {
    /// The model call failed with `E`.
    Model(E),
    /// The model's first step said nothing and called nothing.
    Silent {
        /// The step stopped at its output limit.
        cut: bool,
    },
}

/// A model's failure is the chat's.
impl<E> From<E> for ChatFailure<E> {
    fn from(failure: E) -> Self {
        Self::Model(failure)
    }
}

/// A model's failure reads as the model wrote it.
impl<E: fmt::Display> fmt::Display for ChatFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(failure) => failure.fmt(f),
            Self::Silent { cut: false } => f.write_str("the model answered nothing"),
            Self::Silent { cut: true } => {
                f.write_str("the model reached its output limit before it answered")
            }
        }
    }
}

/// A model's failure is shown as its own, so its cause is the model error's
/// cause, never the error again.
impl<E: std::error::Error> std::error::Error for ChatFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Model(failure) => failure.source(),
            Self::Silent { .. } => None,
        }
    }
}

/// The chat's model, whose failures it lifts into [`ChatFailure`].
#[derive(Debug)]
struct Lifted<M>(M);

impl<M: Model> Model for Lifted<M> {
    type Error = ChatFailure<M::Error>;

    fn measure(&self, request: Request<'_>) -> Result<u64, Self::Error> {
        Ok(self.0.measure(request)?)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Self::Error>> {
        self.0
            .stream(request)
            .map(|chunk| chunk.map_err(ChatFailure::from))
            .boxed()
    }

    fn accepts_images(&self) -> bool {
        self.0.accepts_images()
    }
}

#[cfg(test)]
mod tests;
