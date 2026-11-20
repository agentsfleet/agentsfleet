//! A question asked while children run: the loop ends waiting only once
//! every child has ended and its end is recorded, and the stop or the budget
//! still ends a run whose child never does.

use std::fmt;
use std::future::ready;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::Span;

use super::drive;
use super::script::{ASK_USER, Failure, Script, asking, lock, naming, user};
use crate::DriveState;
use crate::agents::{
    Action, Agents, Caps, Children, Fork, Order, Pause, Raise, Seat, Spawn, Status,
};
use crate::conversation::Conversation;
use crate::drive::Input;
use crate::extension::{Budget, Extension, Plain};
use crate::model::{Chunk, End, Model, Request, TextKind};
use crate::observe::{CallEnd, Observe};
use crate::step::{Ending, Listen, Waiting};
use crate::tool::{ToolFuture, ToolOutput, ToolSpec};
use crate::transcript::{Call, CallRef, Message, Transcript};
use crate::window::Window;

/// The tool that starts a child beside the root and answers its id at once.
const SPAWN: &str = "spawn";
/// How long a child works before it answers.
const CHILD_WORK: Duration = Duration::from_millis(50);
/// The task of a child that never ends, not even when it is stopped.
const NEVER: &str = "never";
/// Longer than any case runs, so a loop that never ends fails its case.
const GUARD: Duration = Duration::from_secs(3_600);
/// The caps the cases run under.
const CAPS: Caps = Caps {
    depth: 1,
    open: 1,
    started: 1,
};

/// What a child's turn fails with; none does here.
#[derive(Debug)]
struct Unfinished;

impl fmt::Display for Unfinished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the child said nothing")
    }
}

/// A child's model: it works a while, then answers its task done; a task
/// that says never is never answered.
#[derive(Debug)]
struct Worker;

impl Model for Worker {
    type Error = Unfinished;

    fn measure(&self, _request: Request<'_>) -> Result<u64, Unfinished> {
        Ok(0)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Unfinished>> {
        let task = match request.messages.last() {
            Some(Message::User(task)) => task.clone(),
            _ => String::new(),
        };
        if task == NEVER {
            return stream::pending().boxed();
        }
        let answer = async move {
            tokio::time::sleep(CHILD_WORK).await;
            Ok(Chunk::Text {
                kind: TextKind::Answer,
                text: format!("{task} done"),
            })
        };
        stream::once(answer)
            .chain(stream::iter([Ok(Chunk::End(End::default()))]))
            .boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// The run the root and its children share: the children, and one log of
/// what both were heard to do, in order.
struct Family {
    agents: Agents,
    heard: Mutex<Vec<String>>,
}

impl Family {
    fn new(agents: Agents) -> Self {
        Self {
            agents,
            heard: Mutex::default(),
        }
    }

    fn note(&self, line: String) {
        lock(&self.heard).push(line);
    }

    fn heard(&self) -> Vec<String> {
        lock(&self.heard).clone()
    }
}

/// One child's turn: the run's worker over the child's transcript, heard as
/// it starts and ends.
struct Child<'f> {
    family: &'f Family,
    id: u64,
    transcript: Transcript,
    stop: CancellationToken,
}

impl Listen for Child<'_> {}

impl Observe<Unfinished> for Child<'_> {
    type Call = ();

    fn step_started(&self, _number: u64) -> Span {
        self.family.note(format!("child {} started", self.id));
        Span::none()
    }

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), _end: CallEnd<'_>) {}

    fn loop_ended(&self, _steps: u64, ending: &Ending<Unfinished>) {
        self.family
            .note(format!("child {} {}", self.id, ending.kind()));
    }
}

impl Conversation for Child<'_> {
    type Failure = Unfinished;
    type Model = Worker;
    type Observer = Self;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(Vec::new())
    }

    fn model(&self) -> &Worker {
        &Worker
    }

    fn observer(&self) -> &Self {
        self
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        Vec::new()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "worker",
            instructions: "",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: "",
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call, _call_ref: &'a CallRef) -> ToolFuture<'a> {
        Box::pin(ready(ToolOutput::not_offered(&call.name)))
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> Unfinished {
        Unfinished
    }
}

impl Raise for Family {
    type Child<'s>
        = Child<'s>
    where
        Self: 's;

    fn agents(&self) -> &Agents {
        &self.agents
    }

    fn child(&self, seat: Seat) -> Child<'_> {
        Child {
            family: self,
            id: seat.id,
            transcript: seat.transcript,
            stop: seat.stop,
        }
    }
}

/// The root loop: a model that spawns a child over `task` and asks a
/// question in one step, and the pause the root took before it ended
/// waiting.
struct Root<'f> {
    family: &'f Family,
    model: Script,
    transcript: Transcript,
    stop: CancellationToken,
    paused: Option<Pause>,
}

impl<'f> Root<'f> {
    fn new(family: &'f Family, task: &str) -> Self {
        let stop = CancellationToken::new();
        let cue = naming([(SPAWN, json!({ "task": task })), asking("Ship it?")]);
        let model = Script {
            steps: Mutex::new([cue].into()),
            stop: stop.clone(),
            ..Script::default()
        };
        Self {
            family,
            model,
            transcript: Transcript::new(vec![user("deploy the fix")]),
            stop,
            paused: None,
        }
    }

    /// Starts a child over the task `arguments` name, beside the root: an
    /// order the loop runs in the root's place.
    fn spawn(&self, arguments: &Value) -> ToolOutput {
        let task = arguments.get("task").and_then(Value::as_str);
        let spawn = Spawn {
            message: task.unwrap_or_default().to_owned(),
            tools: None,
            fork: Fork::None,
        };
        let order = Order::new(self.family.agents.clone(), Action::Spawn(spawn));
        ToolOutput::ordering(order)
    }
}

impl Listen for Root<'_> {}

impl Observe<Failure> for Root<'_> {
    type Call = ();

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), _end: CallEnd<'_>) {}

    fn loop_ended(&self, _steps: u64, ending: &Ending<Failure>) {
        self.family.note(format!("loop {}", ending.kind()));
    }
}

impl Conversation for Root<'_> {
    type Failure = Failure;
    type Model = Script;
    type Observer = Self;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    fn take_input(&mut self) -> impl Future<Output = Vec<Input>> + Send {
        ready(Vec::new())
    }

    fn model(&self) -> &Script {
        &self.model
    }

    fn observer(&self) -> &Self {
        self
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        Vec::new()
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

    fn dispatch<'a>(&'a self, call: &'a Call, _call_ref: &'a CallRef) -> ToolFuture<'a> {
        let output = match call.name.as_str() {
            SPAWN => self.spawn(&call.arguments),
            ASK_USER => {
                let question = call.arguments.get("question").and_then(Value::as_str);
                ToolOutput::asks(question.unwrap_or_default())
            }
            name => ToolOutput::not_offered(name),
        };
        Box::pin(ready(output))
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> Failure {
        Failure::Silent
    }

    async fn quiet(&mut self) {
        self.paused = self.family.agents.quiet().await;
        let paused = self.paused.is_some();
        self.family.note(format!("root quiet, paused {paused}"));
    }
}

/// An extension that adds only its budget, and hears it run out.
struct Bounded {
    budget: Duration,
    spent: bool,
}

impl<'f> Extension<Root<'f>> for Bounded {
    fn budget(&self, _conversation: &Root<'f>) -> Budget {
        Budget::Within(self.budget)
    }

    fn budget_spent(&mut self, _conversation: &mut Root<'f>) {
        self.spent = true;
    }
}

/// The question the root waits on: the second call of its first step.
fn asked() -> Waiting {
    Waiting {
        call_id: "call-1-1".to_owned(),
        question: "Ship it?".to_owned(),
    }
}

#[tokio::test(start_paused = true)]
async fn a_question_waits_for_running_children_before_the_run_ends() {
    let (agents, requests) = Agents::new(CAPS);
    let family = Family::new(agents);
    let mut root = Root::new(&family, "check the build");
    let mut children = Children::new(requests);
    let mut state = DriveState::default();

    let mut plain = Plain;
    let driven = drive(&mut root, &mut plain, &mut state);
    let ended = timeout(GUARD, children.beside(&family, driven)).await;

    assert!(
        matches!(&ended, Ok(Ending::Waiting(on)) if *on == asked()),
        "{ended:?}"
    );
    assert_eq!(
        family.heard(),
        [
            "child 1 started",
            "child 1 answered",
            "root quiet, paused true",
            "loop waiting"
        ],
        "the run ends waiting only after the child's end is recorded"
    );
    let census = root
        .paused
        .as_ref()
        .and_then(|pause| family.agents.census(pause));
    let statuses = census.map(|census| census.children.into_iter().map(|child| child.status));
    assert_eq!(
        statuses.map(Iterator::collect::<Vec<_>>),
        Some(vec![Status::Done("check the build done".to_owned())]),
        "the pause the root holds reads the child's answer"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_while_a_question_waits_on_a_child_ends_the_run_and_keeps_the_question() {
    let (agents, requests) = Agents::new(CAPS);
    let family = Family::new(agents);
    let mut root = Root::new(&family, NEVER);
    let stop = root.stop.clone();
    let mut children = Children::new(requests);
    let mut state = DriveState::default();
    let stopping = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        stop.cancel();
    };

    let mut plain = Plain;
    let driven = drive(&mut root, &mut plain, &mut state);
    let (ended, ()) = tokio::join!(timeout(GUARD, children.beside(&family, driven)), stopping);

    assert!(matches!(ended, Ok(Ending::Stopped)), "{ended:?}");
    assert_eq!(family.heard(), ["child 1 started", "loop stopped"]);
    assert_eq!(
        state.waiting(),
        Some(&asked()),
        "the question is kept for its answer"
    );
    assert!(root.paused.is_none(), "no pause was granted");
}

#[tokio::test(start_paused = true)]
async fn a_question_waiting_on_a_child_that_never_ends_stops_when_the_budget_runs_out() {
    let (agents, requests) = Agents::new(CAPS);
    let family = Family::new(agents);
    let mut root = Root::new(&family, NEVER);
    let mut children = Children::new(requests);
    let mut state = DriveState::default();
    let mut bounded = Bounded {
        budget: Duration::from_secs(30),
        spent: false,
    };

    let driven = drive(&mut root, &mut bounded, &mut state);
    let ended = timeout(GUARD, children.beside(&family, driven)).await;

    assert!(matches!(ended, Ok(Ending::Stopped)), "{ended:?}");
    assert!(bounded.spent, "the extension hears its budget ran out");
    assert_eq!(family.heard(), ["child 1 started", "loop stopped"]);
    assert_eq!(state.waiting(), Some(&asked()));
}
