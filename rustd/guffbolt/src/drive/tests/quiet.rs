//! A question asked while children run: the loop ends waiting only once
//! every child has ended and its end is recorded, and the stop or the budget
//! still ends a run whose child never does.

use std::future::{pending, ready};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::Span;

use super::drive;
use super::script::{ASK_USER, Failure, Script, asking, lock, naming, user};
use crate::DriveState;
use crate::agents::{
    Agents, Caps, ChildFuture, Children, Kind, Narrow, Parent, Pause, Raise, Seat, Spawned, Status,
};
use crate::conversation::Conversation;
use crate::drive::Input;
use crate::extension::{Budget, Extension, Plain};
use crate::model::Request;
use crate::observe::{CallEnd, Observe};
use crate::step::{Ending, Listen, Waiting};
use crate::tool::{ToolFuture, ToolOutput, ToolSpec};
use crate::transcript::{Call, CallRef, Transcript};
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
    running: 1,
    started: 1,
};

/// The tools a child holds: none.
#[derive(Debug, Clone, Copy)]
struct NoTools;

impl Narrow for NoTools {
    fn narrowed<'n, N: AsRef<str>>(&self, names: &'n [N]) -> Result<Self, &'n str> {
        names.first().map_or(Ok(Self), |name| Err(name.as_ref()))
    }

    fn without_agents(&self) -> Self {
        Self
    }
}

/// The run the root and its children share: the children, and one log of
/// what both were heard to do, in order.
struct Family {
    agents: Agents<NoTools>,
    heard: Mutex<Vec<String>>,
}

impl Family {
    fn new(agents: Agents<NoTools>) -> Self {
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

impl Raise for Family {
    type Tools = NoTools;

    fn agents(&self) -> &Agents<NoTools> {
        &self.agents
    }

    fn child<'s>(&'s self, _tools: NoTools, seat: Seat<'s>) -> ChildFuture<'s> {
        Box::pin(async move {
            if seat.task == NEVER {
                pending::<()>().await;
            }
            tokio::time::sleep(CHILD_WORK).await;
            Status::Done(format!("{} done", seat.task))
        })
    }

    fn started(&self, id: u64, _depth: u8) {
        self.note(format!("child {id} started"));
    }

    fn ended(&self, id: u64, _depth: u8, status: Kind, _calls: u64) {
        self.note(format!("child {id} {}", status.as_str()));
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

    /// Starts a child over the task `arguments` name, beside the root.
    fn spawn(&self, arguments: &Value) -> ToolOutput {
        let task = arguments.get("task").and_then(Value::as_str);
        let parent = Parent {
            depth: 0,
            stop: &self.stop,
            tools: &NoTools,
        };
        match self
            .family
            .agents
            .start(parent, task.unwrap_or_default().to_owned(), None)
        {
            Ok(child_id) => {
                let spawned = serde_json::to_string(&Spawned { child_id });
                ToolOutput::succeeded(spawned.unwrap_or_default())
            }
            Err(refused) => ToolOutput::from(refused),
        }
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
            "child 1 done",
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
