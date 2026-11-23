//! The loop with no generic tools: an embedder's own tool, lent the
//! embedder's own state, and every way a call or a step is refused.

use std::future::ready;
use std::sync::Mutex;

use guffbolt::{
    Call, CallEnd, CallOutcome, CallRef, Conversation, Describe, DriveState, Ending, ErrorCode,
    Fitted, Input, Listen, Mark, Message, Observe, Plain, Request, Tool, ToolFuture, ToolOutput,
    ToolSpec, Tools, Transcript, Window, drive,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tracing::Span;

use crate::script::{Failure, Play, Script, call, lock, results};

mod answers;
mod calls;
mod endings;

/// The key every request of this file's loop carries.
const KEY: &str = "plain";

/// What every call is lent: the lines `note` wrote, and the loop's stop.
#[derive(Debug, Default)]
struct Desk {
    notes: Mutex<Vec<String>>,
    stop: CancellationToken,
}

/// Writes down the line it is given; refuses a blank one; asks the person,
/// stops the loop or answers with a process's exit code when told to.
#[derive(Debug)]
struct Note {
    parameters: Value,
}

impl Describe for Note {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "note",
            description: "Writes a line down.",
            parameters: &self.parameters,
        }
    }
}

impl<'n> Tool<&'n Desk> for Note {
    fn call<'a>(&'a self, arguments: &'a Value, cx: &'n Desk) -> ToolFuture<'a>
    where
        &'n Desk: 'a,
    {
        Box::pin(async move {
            if let Some(question) = arguments["ask"].as_str() {
                return ToolOutput::asks(question);
            }
            if arguments["halt"].as_bool() == Some(true) {
                cx.stop.cancel();
                return ToolOutput::succeeded("halted");
            }
            if let Some(code) = arguments["exit"].as_i64() {
                let mut exited = ToolOutput::succeeded(format!("exited {code}"));
                exited.exit_code = i32::try_from(code).ok();
                return exited;
            }
            match arguments["line"].as_str() {
                Some(line) if !line.is_empty() => {
                    lock(&cx.notes).push(line.to_owned());
                    ToolOutput::succeeded(format!("noted {line}"))
                }
                _ => ToolOutput::failed(ErrorCode::new("blank_line"), "a note needs a line"),
            }
        })
    }
}

/// One call's end as the observer heard it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Heard {
    name: String,
    outcome: CallOutcome,
    exit_code: Option<i32>,
    error_code: Option<ErrorCode>,
    mark: Option<Mark>,
}

/// Hears each call's end.
#[derive(Debug, Default)]
struct Watch {
    heard: Mutex<Vec<Heard>>,
}

impl Listen for Watch {}

impl Observe<Failure> for Watch {
    type Call = ();

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), end: CallEnd<'_>) {
        let heard = Heard {
            name: end.call.name.clone(),
            outcome: end.outcome(),
            exit_code: end.exit_code(),
            error_code: end.error_code(),
            mark: end.mark.cloned(),
        };
        lock(&self.heard).push(heard);
    }
}

/// A chat over the scripted model, holding `note`.
struct Chat {
    model: Script,
    tools: Tools<dyn for<'n> Tool<&'n Desk>>,
    desk: Desk,
    watch: Watch,
    pending: Vec<Input>,
    /// What the next window fitting says, once; the whole transcript after.
    fit: Option<Fitted<Failure>>,
    transcript: Transcript,
}

impl Chat {
    fn new(model: Script, opening: &str) -> Self {
        let note: Box<dyn for<'n> Tool<&'n Desk>> = Box::new(Note {
            parameters: json!({"type": "object"}),
        });
        Self {
            model,
            tools: Tools::new(vec![note]),
            desk: Desk::default(),
            watch: Watch::default(),
            pending: Vec::new(),
            fit: None,
            transcript: Transcript::new(vec![Message::User(opening.to_owned())]),
        }
    }

    fn heard(&self) -> Vec<Heard> {
        lock(&self.watch.heard).clone()
    }
}

impl Conversation for Chat {
    type Failure = Failure;
    type Model = Script;
    type Observer = Watch;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(std::mem::take(&mut self.pending))
    }

    fn window(&mut self) -> impl Future<Output = Fitted<Failure>> + Send {
        let fitted = self
            .fit
            .take()
            .unwrap_or_else(|| Fitted::Ready(Window::whole(self.transcript.messages())));
        ready(fitted)
    }

    fn model(&self) -> &Script {
        &self.model
    }

    fn observer(&self) -> &Watch {
        &self.watch
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        self.tools.specs().collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "script",
            instructions: "",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: KEY,
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, _call_ref: &'a CallRef) -> ToolFuture<'a> {
        self.tools.dispatch(&call.name, &call.arguments, &self.desk)
    }

    fn stop(&self) -> &CancellationToken {
        &self.desk.stop
    }

    fn silent(&self, cut: bool) -> Failure {
        Failure::Silent(cut)
    }
}

/// The text of an answered ending.
fn answered(ending: &Ending<Failure>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

/// A task bounded in time: its budget, its record of a spent budget, and
/// an answer it sends back to work once.
struct Timed {
    budget: guffbolt::Budget,
    spent: bool,
    pushed_back: bool,
}

impl guffbolt::Extension<Chat> for Timed {
    fn budget(&self, _chat: &Chat) -> guffbolt::Budget {
        self.budget
    }

    fn budget_spent(&mut self, _chat: &mut Chat) {
        self.spent = true;
    }

    fn on_answer(
        &mut self,
        _chat: &mut Chat,
        answer: &str,
    ) -> impl Future<Output = guffbolt::Answer<Failure>> + Send {
        let decided = if self.pushed_back || answer.contains("done") {
            guffbolt::Answer::Accept
        } else {
            self.pushed_back = true;
            guffbolt::Answer::Continue("keep going until it is done".to_owned())
        };
        ready(decided)
    }
}
