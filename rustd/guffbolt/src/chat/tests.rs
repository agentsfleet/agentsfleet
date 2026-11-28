//! A chat over a scripted model: it answers, runs its tools, fails with its
//! model, waits on a question and stops.

use std::collections::VecDeque;
use std::error::Error as _;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use serde_json::{Value, json};

use super::{Chat, ChatFailure};
use crate::conversation::Conversation;
use crate::step::Ending;
use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, Tools};
use guffbolt_ai::{Call, Chunk, End, Message, Model, Replay, Request, TextKind, ToolSpec};

const INSTRUCTIONS: &str = "Answer in one sentence.";
const QUESTION: &str = "Which crate parses TOML in Rust?";

/// Why a scripted model call failed: its provider was down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Down;

impl fmt::Display for Down {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the provider is down")
    }
}

impl std::error::Error for Down {}

/// What one scripted model call streams.
type Played = Vec<Result<Chunk, Down>>;

/// Each request's instructions and offered tool names, in order.
type Sent = Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// A model that plays its steps in order, failing once they run out, and
/// keeps each request's instructions and offered tool names.
#[derive(Debug, Default)]
struct Script {
    steps: Mutex<VecDeque<Played>>,
    sent: Sent,
}

impl Script {
    fn new(steps: impl IntoIterator<Item = Played>) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
            sent: Arc::default(),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Model for Script {
    type Error = Down;

    fn measure(&self, request: Request<'_>) -> Result<u64, Down> {
        if request.messages.is_empty() {
            return Err(Down);
        }
        Ok(u64::try_from(request.messages.len()).unwrap_or(u64::MAX))
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Down>> {
        let names = request.tools.iter().map(|tool| tool.name.to_owned());
        lock(&self.sent).push((request.instructions.to_owned(), names.collect()));
        let step = lock(&self.steps).pop_front();
        stream::iter(step.unwrap_or_else(|| vec![Err(Down)])).boxed()
    }

    fn accepts_images(&self) -> bool {
        true
    }
}

/// A step answering `text`.
fn answering(text: &str) -> Played {
    vec![
        Ok(Chunk::Text {
            kind: TextKind::Answer,
            text: text.to_owned(),
        }),
        Ok(Chunk::End(End::default())),
    ]
}

/// A step calling `name` with `arguments`.
fn calling(name: &str, arguments: Value) -> Played {
    let call = Call {
        id: "call-1".to_owned(),
        name: name.to_owned(),
        arguments,
    };
    vec![Ok(Chunk::Call(call)), Ok(Chunk::End(End::default()))]
}

/// `note`: counts its runs, and answers with the line it was given, or asks
/// the person the question it was given.
#[derive(Debug)]
struct Note {
    parameters: Value,
    ran: Arc<AtomicUsize>,
}

impl Note {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let ran = Arc::new(AtomicUsize::new(0));
        let parameters = json!({ "type": "object" });
        let held = Arc::clone(&ran);
        (Self { parameters, ran }, held)
    }
}

impl Describe for Note {
    fn guidance(&self) -> &'static str {
        "Use note to record a requested line."
    }

    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "note",
            description: "Writes a line down.",
            parameters: &self.parameters,
        }
    }
}

impl Tool<()> for Note {
    fn call<'a>(&'a self, arguments: &'a Value, (): ()) -> ToolFuture<'a>
    where
        (): 'a,
    {
        self.ran.fetch_add(1, Ordering::SeqCst);
        let text = |key| arguments.get(key).and_then(Value::as_str);
        let output = text("ask").map_or_else(
            || ToolOutput::succeeded(format!("Noted: {}", text("line").unwrap_or_default())),
            ToolOutput::asks,
        );
        Box::pin(std::future::ready(output))
    }
}

fn noting(note: Note) -> Tools<dyn Tool<()>> {
    Tools::new(vec![Box::new(note)])
}

fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

/// The text of an answered ending.
fn answered<E>(ending: &Ending<E>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

#[tokio::test]
async fn a_chat_answers_one_prompt() {
    let model = Script::new([answering("toml")]);
    let sent = Arc::clone(&model.sent);
    let mut chat = Chat::new(model, INSTRUCTIONS);
    let ending = chat.prompt(QUESTION).await;
    assert_eq!(answered(&ending), Some("toml"));
    assert_eq!(
        chat.transcript().messages(),
        [Message::User(QUESTION.to_owned()), said("toml")],
        "the transcript holds the prompt and the answer"
    );
    assert_eq!(*lock(&sent), [(INSTRUCTIONS.to_owned(), Vec::new())]);
}

#[tokio::test]
async fn a_chat_runs_a_tool_and_answers_with_its_output() {
    let (note, ran) = Note::new();
    let model = Script::new([
        calling("note", json!({ "line": "toml" })),
        answering("Noted: toml"),
    ]);
    let sent = Arc::clone(&model.sent);
    let mut chat = Chat::new(model, INSTRUCTIONS).with_tools(noting(note));
    let ending = chat.prompt("Note the crate down.").await;
    assert_eq!(answered(&ending), Some("Noted: toml"));
    assert_eq!(ran.load(Ordering::SeqCst), 1, "the tool ran once");
    let messages = chat.transcript().messages();
    let result = messages.iter().position(
        |message| matches!(message, Message::ToolResult { output, .. } if output == "Noted: toml"),
    );
    assert_eq!(
        result.map(|at| at + 2),
        Some(messages.len()),
        "the answer follows the tool's result"
    );
    let expected = (
        format!("{INSTRUCTIONS}\n\nUse note to record a requested line."),
        vec!["note".to_owned()],
    );
    assert_eq!(
        *lock(&sent),
        [expected.clone(), expected],
        "both requests carry the offered tool and its guidance"
    );
}

#[tokio::test]
async fn a_chat_whose_model_fails_ends_failed() {
    let mut chat = Chat::new(Script::new([vec![Err(Down)]]), INSTRUCTIONS);
    let ending = chat.prompt(QUESTION).await;
    assert!(
        matches!(ending, Ending::Failed(ChatFailure::Model(Down))),
        "the ending carries the model's error: {ending:?}"
    );
}

#[tokio::test]
async fn a_chat_whose_model_says_nothing_ends_silent() {
    let cut = End {
        cut: true,
        ..End::default()
    };
    let steps = [
        vec![Ok(Chunk::End(End::default()))],
        vec![Ok(Chunk::End(cut))],
    ];
    let mut chat = Chat::new(Script::new(steps), INSTRUCTIONS);
    for expected in [false, true] {
        let ending = chat.prompt(QUESTION).await;
        assert!(
            matches!(ending, Ending::Failed(ChatFailure::Silent { cut }) if cut == expected),
            "a blank first step is a failure: {ending:?}"
        );
    }
}

#[tokio::test]
async fn a_second_prompt_follows_the_first_answer_and_answers_alone() {
    let model = Script::new([answering("toml"), answering("serde")]);
    let mut chat = Chat::new(model, INSTRUCTIONS);
    chat.prompt(QUESTION).await;
    let ending = chat.prompt("And JSON?").await;
    assert_eq!(
        answered(&ending),
        Some("serde"),
        "only this prompt's answer"
    );
    assert_eq!(
        chat.transcript().messages(),
        [
            Message::User(QUESTION.to_owned()),
            said("toml"),
            Message::User("And JSON?".to_owned()),
            said("serde"),
        ]
    );
}

#[tokio::test]
async fn a_question_waits_and_the_next_prompt_answers_it() {
    let (note, ran) = Note::new();
    let model = Script::new([
        calling("note", json!({ "ask": "Which format?" })),
        answering("toml it is"),
    ]);
    let mut chat = Chat::new(model, INSTRUCTIONS).with_tools(noting(note));
    let first = chat.prompt("Pick a format.").await;
    let question = match &first {
        Ending::Waiting(waiting) => Some(waiting.question.as_str()),
        Ending::Answered(_) | Ending::Failed(_) | Ending::Stopped => None,
    };
    assert_eq!(question, Some("Which format?"));
    let second = chat.prompt("TOML").await;
    assert_eq!(answered(&second), Some("toml it is"));
    assert_eq!(ran.load(Ordering::SeqCst), 1);
    let answer = chat.transcript().messages().iter().any(|message| {
        matches!(message, Message::ToolResult { call_id, output, .. }
            if call_id == "call-1" && output == "TOML")
    });
    assert!(answer, "the prompt is the asking call's result");
}

#[tokio::test]
async fn a_stopped_chat_ends_stopped_before_its_model_answers() {
    let model = Script::new([answering("toml")]);
    let sent = Arc::clone(&model.sent);
    let mut chat = Chat::new(model, INSTRUCTIONS);
    chat.stop().clone().cancel();
    let ending = chat.prompt(QUESTION).await;
    assert!(matches!(ending, Ending::Stopped), "{ending:?}");
    assert!(lock(&sent).len() <= 1, "no step after the stop");
}

#[test]
fn the_chat_model_measures_and_reads_images_as_its_own() {
    let chat = Chat::new(Script::default(), INSTRUCTIONS);
    let model = chat.model();
    let messages = [Message::User(QUESTION.to_owned())];
    let request = |messages| Request {
        model: "",
        instructions: INSTRUCTIONS,
        messages,
        tools: &[],
        cache_key: "",
        max_output_tokens: None,
        single_attempt: false,
    };
    assert_eq!(model.measure(request(&messages)).ok(), Some(1));
    assert!(matches!(
        model.measure(request(&[])),
        Err(ChatFailure::Model(Down))
    ));
    assert!(model.accepts_images());
}

#[test]
fn a_chat_failure_reads_as_its_model_wrote_it() {
    let failure = ChatFailure::from(Down);
    assert_eq!(failure.to_string(), "the provider is down");
    assert!(failure.source().is_none(), "the model error has no cause");
    let silent = ChatFailure::<Down>::Silent { cut: false };
    assert_eq!(silent.to_string(), "the model answered nothing");
    assert!(silent.source().is_none());
    let cut = ChatFailure::<Down>::Silent { cut: true };
    assert_eq!(
        cut.to_string(),
        "the model reached its output limit before it answered"
    );
}
