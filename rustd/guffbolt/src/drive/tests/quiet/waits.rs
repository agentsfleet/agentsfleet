//! A root that spawns a child and asks in one step, and the three ways its
//! wait ends.

use super::*;

/// The root loop: a model that spawns a child over `task` and asks a
/// question in one step. It adds nothing for the wait: the loop waits for
/// the children itself.
struct Root<'f> {
    family: &'f Family,
    model: Script,
    transcript: Transcript,
    stop: CancellationToken,
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
    let pause = children.end_all(&family.agents);

    assert!(
        matches!(&ended, Ok(Ending::Waiting(on)) if *on == asked()),
        "{ended:?}"
    );
    assert_eq!(
        family.heard(),
        ["child 1 started", "child 1 answered", "loop waiting"],
        "the run ends waiting only after the child's end is recorded"
    );
    let census = pause.as_ref().and_then(|pause| family.agents.census(pause));
    let statuses = census.map(|census| census.children.into_iter().map(|child| child.status));
    assert_eq!(
        statuses.map(Iterator::collect::<Vec<_>>),
        Some(vec![Status::Done("check the build done".to_owned())]),
        "the child's answer outlives the run's end"
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
