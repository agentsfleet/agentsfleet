# guffbolt

`guffbolt` is an agent harness for Rust. It is used by [agentsfleet](https://github.com/agentsfleet/agentsfleet), whose runner drives every agent run through it.

`guffbolt` runs an agent loop's steps in a fixed order. You supply the model, the conversation's input and window, and the tools.

`guffbolt` makes each model call, reads the step, runs its calls and keeps the transcript. It decides when each step runs and when the loop ends, and how a tool call is looked up and refused. It tells your observer as each model call and tool call starts and ends.

## Prerequisites

- **Rust** with edition 2024, at or above the `rust-version` in `Cargo.toml`.
- **The crate.** It is not on crates.io yet, so depend on it from the agentsfleet repository.
- **`Send` futures.** Each async method you write must return a `Send` future, because the traits require it. Your conversation is shared with every call it lends, so it must be `Sync` as well.
- **Tokio, for a time budget.** `Budget::Within` uses Tokio's timer, so that loop must run inside a Tokio runtime with time enabled.

```toml
[dependencies]
guffbolt = { git = "https://github.com/agentsfleet/agentsfleet" }
```

## How a loop runs

```text
drive(conversation, extension, state) ──► Ending: Answered | Failed | Stopped | Waiting

every step:  take input ─► before_model ─► window ─► model call ─► after_step
no calls:    on_answer ─► remember ─► take input again; new input runs one more step
calls:       run them ─► remember ─► after_group
a question:  run the step's calls ─► remember ─► quiet ─► Waiting
```

A message that arrives while the model works joins the next model call. `guffbolt` never cuts a call short for it.

Before each model call, your `window` says what that call sends of the transcript. `guffbolt` makes the model call itself. It sends your `request` to your `Model` and reads the stream into a `Step`.

`remember` puts the step and its results into the `Transcript`. Your `scrub` runs first and hides what must not reach a model again.

## A plain chat with one tool

Implement `Model` for your provider, a tool for what the model may do, and `Conversation` for your chat. Then call `drive` with `Plain`, which adds nothing to the loop.

```rust
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use guffbolt::{
    Call, Chunk, Conversation, Describe, DriveState, End, Ending, Message, Model, Plain, Request,
    TextKind, Tool, ToolFuture, ToolOutput, ToolSpec, Tools, Transcript, Unobserved, Window, drive,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// The lines a chat keeps, lent to the tool that writes them.
type Notes = Mutex<Vec<String>>;

/// Writes down the line the model gives it.
#[derive(Debug)]
struct Note {
    parameters: Value,
}

impl Describe for Note {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec { name: "note", description: "Writes a line down.", parameters: &self.parameters }
    }
}

impl<'n> Tool<&'n Notes> for Note {
    fn call<'a>(&'a self, arguments: &'a Value, cx: &'n Notes) -> ToolFuture<'a>
    where
        &'n Notes: 'a,
    {
        // The work runs when the loop polls the call, inside its span.
        Box::pin(async move {
            let line = arguments["line"].as_str().unwrap_or_default().to_owned();
            cx.lock().unwrap().push(line.clone());
            ToolOutput::succeeded(format!("Noted: {line}"))
        })
    }
}

/// A model that notes down what it is told, then says what it noted.
#[derive(Debug)]
struct Notetaker;

impl Model for Notetaker {
    type Error = String;

    fn measure(&self, _request: Request<'_>) -> Result<u64, String> {
        // Count the request's tokens the way your provider does here.
        Ok(0)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, String>> {
        // Send the request to your provider here, and map its stream.
        let chunk = match request.messages.last() {
            Some(Message::User(said)) => Chunk::Call(Call {
                id: "call-1".to_owned(),
                name: "note".to_owned(),
                arguments: json!({ "line": said }),
            }),
            Some(Message::ToolResult { output, .. }) => {
                Chunk::Text { kind: TextKind::Answer, text: output.clone() }
            }
            _ => Chunk::Text { kind: TextKind::Answer, text: String::new() },
        };
        stream::iter([Ok(chunk), Ok(Chunk::End(End::default()))]).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// A chat: its model and its tool, the lines the tool wrote, its transcript,
/// the messages people send while it runs, and its stop.
struct Chat {
    model: Notetaker,
    tools: Tools<dyn for<'n> Tool<&'n Notes>>,
    notes: Notes,
    transcript: Transcript,
    inbox: Mutex<Receiver<String>>,
    stop: CancellationToken,
}

impl Conversation for Chat {
    type Failure = String;
    type Model = Notetaker;
    // Watch the loop's work here; `Unobserved` hears nothing.
    type Observer = Unobserved;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    async fn take_input(&mut self) -> Vec<String> {
        self.inbox.get_mut().map(|inbox| inbox.try_iter().collect()).unwrap_or_default()
    }

    // `window` keeps its default, which sends the whole transcript.

    fn model(&self) -> &Notetaker {
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
            model: "notetaker-1",
            instructions: "Note down what you are told.",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: "chat-1",
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call) -> ToolFuture<'a> {
        // Each call is lent the chat's notes; a name `tools` holds no tool for is refused.
        self.tools.dispatch(&call.name, &call.arguments, &self.notes)
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> String {
        "the model answered nothing".to_owned()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let note = Note {
        parameters: json!({
            "type": "object",
            "properties": { "line": { "type": "string" } },
            "required": ["line"]
        }),
    };
    let (send, inbox) = channel();
    let mut chat = Chat {
        model: Notetaker,
        tools: Tools::new(vec![Box::new(note)]),
        notes: Mutex::default(),
        transcript: Transcript::new(vec![Message::User("Deploy the fix.".to_owned())]),
        inbox: Mutex::new(inbox),
        stop: CancellationToken::new(),
    };
    send.send("Use staging.".to_owned()).unwrap();

    match drive(&mut chat, &mut Plain, &mut DriveState::default()).await {
        // The model read both messages, joined into one, and noted them.
        Ending::Answered(text) => assert_eq!(text, "Noted: Deploy the fix.\n\nUse staging."),
        Ending::Failed(why) => panic!("{why}"),
        Ending::Stopped => panic!("nothing stopped this loop"),
        Ending::Waiting(waiting) => panic!("no call asked: {waiting:?}"),
    }
    assert_eq!(chat.notes.into_inner().unwrap(), ["Deploy the fix.\n\nUse staging."]);
}
```

`take_input` runs before every model call and once after an answer. A message sent while a tool runs reaches the next call. One sent while the model writes its answer runs one more step, and reaches the model after that answer.

Cancel the token `stop` returns to stop the loop. `guffbolt` races each model call and each tool call against it. `called` runs after each call that answered, and defaults to doing nothing.

## Fitting the window

Before every model call, `guffbolt` asks your conversation for that call's `Window`, the way pi runs `transformContext`. A window holds the messages the call sends and the most the call may write. The default `window` sends the whole transcript.

Override `window` to keep a long conversation inside the model's context. Measure a request with `Model::measure`, then summarize or drop the oldest messages, and keep each tool call with its results. Your `request` sends the window's messages and its output limit.

`window` answers `Fitted::Ready` with the window, `Fitted::Failed` to fail the run, or `Fitted::Stopped` when the stop cut the fit short. A window lives for one call. The `Transcript` keeps every message, so the next call can fit it again from the start.

## Watching a loop

Your observer hears the loop's work while it happens. `guffbolt` opens, runs and ends each tool call itself, so `call_ended` comes once per call. A call the stop cuts short, or one whose loop is dropped, ends `Interrupted`.

```rust
use std::sync::Mutex;

use guffbolt::{Call, CallEnd, Listen, Observe, TextKind};
use tracing::Span;

/// Shows what the model writes, and notes how each tool call ended.
#[derive(Default)]
struct Watch {
    ended: Mutex<Vec<String>>,
}

impl Listen for Watch {
    fn text(&self, _kind: TextKind, text: &str) {
        print!("{text}");
    }
}

impl<F> Observe<F> for Watch {
    type Call = ();

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), tracing::info_span!("tool_call"))
    }

    fn call_ended(&self, _kept: (), end: CallEnd<'_>) {
        let line = format!("{} {:?} after {:?}", end.call.name, end.outcome(), end.duration);
        self.ended.lock().unwrap().push(line);
    }
}
```

| Event | When it runs | What an embedder does there |
|---|---|---|
| `step_started` | before each model call builds its request | log it, and open the span its stream is read in |
| `text` and `usage` | as each chunk of the stream arrives | stream the text live and meter the spend |
| `step_ended` | once the step is read, fails or is stopped | count it, and record what it spent |
| `call_started` | before each tool call runs or is refused | number it, and open the span it runs in |
| `call_ended` | once per call, however it ended | record how it ended, and mask what the model must not read |
| `loop_ended` | when `drive` returns | read how the loop ended |

Each event takes `&self`, and `guffbolt` waits on none of them. `call_started` returns what you keep until the call ends, and the span the call runs in. The model reads a call's output after `call_ended` returns, so mask a secret there.

## A long-running task

A task with a goal adds its rules to the same loop through an `Extension`. Every hook defaults to adding nothing, so a task overrides only the hooks it needs.

```rust
use std::sync::mpsc::Receiver;
use std::time::Duration;

use guffbolt::{Answer, Budget, Conversation, DriveState, Ending, Extension, drive};

/// A task: at most a minute of work, what its owner adds while it runs, and
/// no answer until the goal is met.
struct Task {
    goal: &'static str,
    budget: Duration,
    added: Receiver<String>,
}

impl<C: Conversation> Extension<C> for Task {
    fn budget(&self, _conversation: &C) -> Budget {
        Budget::Within(self.budget)
    }

    async fn before_model(
        &mut self,
        conversation: &mut C,
        _number: u64,
        _state: &DriveState,
    ) -> Option<Ending<C::Failure>> {
        // What the owner added since the last call reaches the model with it.
        for added in self.added.try_iter() {
            conversation.transcript().join(added);
        }
        None
    }

    async fn on_answer(&mut self, _conversation: &mut C, answer: &str) -> Answer<C::Failure> {
        if answer.contains(self.goal) {
            Answer::Accept
        } else {
            Answer::Continue(format!("Keep going: the goal is \"{}\".", self.goal))
        }
    }
}

/// Runs any conversation as a task.
async fn run_task<C: Conversation>(conversation: &mut C, added: Receiver<String>) -> Ending<C::Failure> {
    let mut task = Task { goal: "deployed", budget: Duration::from_secs(60), added };
    drive(conversation, &mut task, &mut DriveState::default()).await
}
```

| Hook | When it runs | What a task can do there |
|---|---|---|
| `budget` | once, before the first step | bound the whole run's time |
| `before_model` | before every model call | read the task's new inputs, or end the run |
| `after_step` | when a model step arrives, before its tools run | refuse a step, such as one cut at its output limit |
| `on_answer` | when the model answers without calling a tool | accept, end the run, or send the model back to work |
| `after_group` | after a step's tool calls ran | check the goal after each batch of work |
| `budget_spent` | when the budget runs out | drop what the run can no longer finish |

`DriveState` carries the accepted answers and the completed steps from one run of a loop to the next. Save it with the transcript to resume a task later. A run of no special kind installs `Plain`.

## Tools

A tool implements `Describe`, which gives the model its name, description and parameters' JSON Schema. It also implements `Tool<Cx>`, which runs one call with the context `Cx` your loop lends it. `Cx` is yours, such as the state the calls share, or the loop itself.

`Tools` holds one loop's tools, offers them to the model in order and dispatches a call by name. A call to a name it holds no tool for is answered `[tool_not_offered]`. `Tools::hold` keeps a tool callable without offering it.

A tool answers with a `ToolOutput`: the text the model reads, an exit code, an `ErrorCode`, an `Image` to show the model and a question it asks the person. `guffbolt` answers a call from a step cut at the output limit with `ToolOutput::cut`, `[output_limit_reached]`, and never runs it.

## Sub-agents

A tool call can start a child, which is another loop with its own transcript and a narrower set of its parent's tools. `guffbolt::agents` keeps every child's state, inbox and stop, and refuses one past the run's `Caps`.

```rust
use std::future::ready;

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use guffbolt::agents::{
    Agents, Caps, ChildFuture, Children, Narrow, Parent, Raise, Seat, Status, Tether,
};
use guffbolt::{
    Call, Chunk, Conversation, DriveState, End, Ending, Message, Model, Plain, Request, TextKind,
    ToolFuture, ToolOutput, ToolSpec, Transcript, Unobserved, Window, drive,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// The tool that hands a task to a child and answers with the child's answer.
const DELEGATE: &str = "delegate";

/// A model that delegates the task it is given when it may, does it when it
/// may not, and passes a child's answer on.
#[derive(Debug)]
struct Lead;

impl Model for Lead {
    type Error = String;

    fn measure(&self, _request: Request<'_>) -> Result<u64, String> {
        Ok(0)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, String>> {
        let may_delegate = request.tools.iter().any(|tool| tool.name == DELEGATE);
        let chunk = match request.messages.last() {
            Some(Message::User(task)) if may_delegate => Chunk::Call(Call {
                id: "call-1".to_owned(),
                name: DELEGATE.to_owned(),
                arguments: json!({ "task": task }),
            }),
            Some(Message::User(task)) => answer(format!("Done: {task}")),
            Some(Message::ToolResult { output, .. }) => answer(output.clone()),
            _ => answer(String::new()),
        };
        stream::iter([Ok(chunk), Ok(Chunk::End(End::default()))]).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

fn answer(text: String) -> Chunk {
    Chunk::Text { kind: TextKind::Answer, text }
}

/// The names of the tools a loop holds.
#[derive(Debug, Clone)]
struct Names(Vec<&'static str>);

impl Narrow for Names {
    fn narrowed<'n, N: AsRef<str>>(&self, names: &'n [N]) -> Result<Self, &'n str> {
        let mut kept = Vec::new();
        for name in names.iter().map(AsRef::as_ref) {
            kept.push(*self.0.iter().find(|held| **held == name).ok_or(name)?);
        }
        Ok(Self(kept))
    }

    fn without_agents(&self) -> Self {
        Self(self.0.iter().copied().filter(|name| *name != DELEGATE).collect())
    }
}

/// What every loop of one run shares: its children, and the tool's parameters.
struct Run {
    agents: Agents<Names>,
    parameters: Value,
}

/// One loop of the run: the root, or a child.
struct Chat<'s> {
    run: &'s Run,
    depth: u8,
    tools: Names,
    transcript: Transcript,
    /// What a child's parent sends it; the root has none.
    tether: Option<Tether<'s>>,
    stop: CancellationToken,
}

impl Conversation for Chat<'_> {
    type Failure = String;
    type Model = Lead;
    type Observer = Unobserved;

    fn transcript(&mut self) -> &mut Transcript {
        &mut self.transcript
    }

    async fn take_input(&mut self) -> Vec<String> {
        self.tether.as_mut().map(Tether::sent).unwrap_or_default()
    }

    fn model(&self) -> &Lead {
        &Lead
    }

    fn observer(&self) -> &Unobserved {
        &Unobserved
    }

    fn offered(&self) -> Vec<ToolSpec<'_>> {
        let delegate = ToolSpec {
            name: DELEGATE,
            description: "Hands a task to a child agent and answers with its answer.",
            parameters: &self.run.parameters,
        };
        self.tools.0.iter().filter(|name| **name == DELEGATE).map(|_| delegate).collect()
    }

    fn request<'a>(&'a self, window: &'a Window, tools: &'a [ToolSpec<'a>]) -> Request<'a> {
        Request {
            model: "lead-1",
            instructions: "Delegate what you can.",
            messages: &window.messages,
            tools,
            hosted: &[],
            cache_key: "run-1",
            max_output_tokens: window.max_output_tokens,
            single_attempt: false,
        }
    }

    fn dispatch<'a>(&'a self, call: &'a Call) -> ToolFuture<'a> {
        if call.name != DELEGATE || !self.tools.0.contains(&DELEGATE) {
            return Box::pin(ready(ToolOutput::not_offered(&call.name)));
        }
        let task = call.arguments["task"].as_str().unwrap_or_default().to_owned();
        let parent = Parent { depth: self.depth, stop: &self.stop, tools: &self.tools };
        match self.run.agents.start(parent, task, None) {
            // The child runs beside the root, and this call waits for its answer.
            Ok(child) => Box::pin(self.run.agents.delegated(child)),
            Err(refused) => Box::pin(ready(ToolOutput::from(refused))),
        }
    }

    fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    fn silent(&self, _cut: bool) -> String {
        "the model answered nothing".to_owned()
    }

    async fn quiet(&mut self) {
        // Before the run waits on a question, every child ends first. Only the
        // root waits: the run's children include every child loop.
        if self.depth == 0 {
            let _paused = self.run.agents.quiet().await;
        }
    }
}

impl Raise for Run {
    type Tools = Names;

    fn agents(&self) -> &Agents<Names> {
        &self.agents
    }

    fn child<'s>(&'s self, tools: Names, seat: Seat<'s>) -> ChildFuture<'s> {
        Box::pin(async move {
            let mut child = Chat {
                run: self,
                depth: seat.depth,
                tools,
                transcript: Transcript::new(vec![Message::User(seat.task)]),
                tether: Some(seat.tether),
                stop: seat.stop,
            };
            match drive(&mut child, &mut Plain, &mut DriveState::default()).await {
                Ending::Answered(text) => Status::Done(text),
                Ending::Failed(why) => Status::Failed(why),
                Ending::Stopped | Ending::Waiting(_) => Status::Interrupted,
            }
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // A child of the root sits at the depth cap, so it holds no `delegate` of its own.
    let (agents, requests) = Agents::new(Caps { depth: 1, running: 2, started: 4 });
    let parameters = json!({
        "type": "object",
        "properties": { "task": { "type": "string" } },
        "required": ["task"]
    });
    let run = Run { agents, parameters };
    let mut root = Chat {
        run: &run,
        depth: 0,
        tools: Names(vec![DELEGATE]),
        transcript: Transcript::new(vec![Message::User("Check the build.".to_owned())]),
        tether: None,
        stop: CancellationToken::new(),
    };
    let mut children = Children::new(requests);
    let ending =
        children.beside(&run, drive(&mut root, &mut Plain, &mut DriveState::default())).await;
    // Every child still running ends interrupted here, with the root.
    children.end_all(&run.agents);

    match ending {
        // The root handed the task to a child and answered with the child's answer.
        Ending::Answered(text) => assert_eq!(text, "Done: Check the build."),
        Ending::Failed(why) => panic!("{why}"),
        Ending::Stopped => panic!("nothing stopped this run"),
        Ending::Waiting(waiting) => panic!("no call asked: {waiting:?}"),
    }
}
```

`Children::beside` polls the root first, and every child whenever the root waits, all in the root's own future. No child is spawned onto the runtime, so a run that ends takes its children with it. `Children::end_all` drops what is left, and each child still running ends interrupted.

Implement `Raise` to say which loop a child runs, and to hear each one start and end. `Agents::start` narrows the child's tools through `Narrow`, and a child at the depth cap holds none that start children. Your tools that steer children call `Agents`, and answer with its shapes: `Spawned`, `State`, `Accepted` and `Listed`.

A `Pause` holds new children back, and is granted only while no child runs, unwinds or waits to start. Read `Agents::census` under it to save the children, and bring the ended ones back with `Agents::restored`. `Agents::pause` is refused while a child runs. `Agents::quiet` waits until every child has ended, while `Children::beside` keeps polling them, and then pauses.

## Media

`guffbolt::input` types a message as text plus image, audio, video or document parts. Your `MediaReader` steps each part into text the model reads. `Unread` names each part to the model until you read that kind.

## Asking the person

A call that answers `ToolOutput::asks` asks the person a question. The loop runs the rest of that step's calls and waits for your `Conversation::quiet`. Then it ends `Ending::Waiting` with the asking call's id and its question, and nothing waits on the person inside the run. Deliver the question however your product reaches people. When the answer arrives, `DriveState::resume` makes it that call's result, and the next `drive` goes on from there. A state saved while waiting resumes the same way later, in another process.

`quiet` defaults to returning at once. A root whose run starts children waits there with `Agents::quiet`, so each child ends and its answer is recorded before the run waits. The loop races that wait against its stop, and its budget bounds it. A stop that cuts the wait short ends the loop `Ending::Stopped` and keeps the question in the state. A second question in the same step is answered `[question_pending]`.

```rust
use guffbolt::{Conversation, DriveState, Ending, Plain, drive};

/// Drives `chat` to its end, putting each question to the person through
/// `ask` and resuming with the answer.
async fn answered<C: Conversation>(
    chat: &mut C,
    state: &mut DriveState,
    ask: impl Fn(&str) -> String,
) -> Ending<C::Failure> {
    loop {
        match drive(chat, &mut Plain, state).await {
            Ending::Waiting(waiting) => {
                let answer = ask(&waiting.question);
                state.resume(chat.transcript(), answer);
            }
            ending => return ending,
        }
    }
}
```

## The `tools` feature

The `tools` feature adds tools that work on a machine and on the network. Each tool reaches the outside world only through a seam you fill: `Env` for processes and files, `Outbound` for network requests, `MemoryStore` for memories. `Local` fills all three for a harness on your own machine.

```toml
[dependencies]
guffbolt = { git = "https://github.com/agentsfleet/agentsfleet", features = ["tools"] }
```

| Seam | Tools | Local default |
|---|---|---|
| `Env` | `shell`, `exec_command`, `write_stdin`, `git`, `file_read`, `file_read_hashed`, `file_write`, `file_append`, `file_delete`, `file_edit`, `file_edit_hashed`, `apply_patch`, `image`, `ls`, `find`, `grep`, `browser_open`, `browser`, `screenshot` | `LocalEnv`: this host's processes, the files under one directory |
| `Outbound` | `http_request`, `web_fetch`, `pushover`, MCP servers over HTTP | `LocalOutbound`: HTTPS only, to the hosts an `Allowlist` names |
| `MemoryStore` | `memory_store`, `memory_recall`, `memory_list`, `memory_forget` | `LocalMemory`: one process's map |
| none | `update_plan`, `ask_user`, `tool_search`, `current_time`, `sleep` | |

A tool is a `Handler`: its arguments are a type, and the model is told that type's JSON Schema. `Typed` makes a handler a `Tool` lent a `tools::Context` per call, and `tools::standard()` boxes every tool that needs nothing else.

```rust,no_run
use guffbolt::Tool as _;
use guffbolt::tools::outbound::Allowlist;
use guffbolt::tools::process::Shell;
use guffbolt::tools::{Local, Typed};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let local = Local::new("run-1", ".", Allowlist::Hosts(vec!["docs.rs".to_owned()]))?;
    let shell = Typed::new(Shell);

    let output = shell.call(&json!({ "command": "echo hi" }), local.context()).await;
    assert_eq!(output.text, "hi\n");
    Ok(())
}
```

`LocalEnv` runs every command as the user that runs the harness, with that user's reach. Run untrusted work in a sandbox, and give its tools an `Env` that sends each call there. Never give them `LocalEnv` on a host that holds a secret a command must not read, such as a provider key.

### Env

An `Env` starts a process and hands back its events, writes to it and kills it, and reads, writes, appends, deletes and lists files under one root. Every path is relative to `Env::root`, and the environment refuses one that leaves it, by name or through a link. A process's unread output is kept as its first and last 512 KiB, and the middle is counted. `exec_command` and `write_stdin` keep a process open across calls in the run's `Sessions`. Close them with `Sessions::close_all` when the run ends.

### Outbound

Every network request goes through `Outbound`, which admits it, rewrites it or refuses it, and masks the secrets it placed out of the answer. A tool that places a credential itself asks `Outbound::secret`, and an MCP client asks `Outbound::connect` before it connects. A refusal is the code and sentence the model reads.

```rust
use std::pin::Pin;

use guffbolt::tools::code;
use guffbolt::tools::outbound::{Admitted, Outbound, OutboundFuture, Refused, Request, Response};

/// A hook for a run that must not reach the network.
#[derive(Debug)]
struct Offline;

impl Outbound for Offline {
    fn send<'a>(&'a self, tool: &'a str, request: Request) -> OutboundFuture<'a, Response> {
        let why = format!("{tool} may not reach {}: this run is offline", request.url);
        Box::pin(std::future::ready(Err(Refused::new(code::HOST_NOT_ALLOWED, why))))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(std::future::ready(text))
    }

    fn secret<'a>(&'a self, _tool: &'a str, name: &'a str, _field: &'a str) -> OutboundFuture<'a, String> {
        let why = format!("this run holds no secret {name}");
        Box::pin(std::future::ready(Err(Refused::new(code::SECRET_NOT_FOUND, why))))
    }

    fn connect<'a>(&'a self, _tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        let why = format!("{url}: this run is offline");
        Box::pin(std::future::ready(Err(Refused::new(code::HOST_NOT_ALLOWED, why))))
    }
}
```

### MemoryStore

The memory tools read and write a `MemoryStore`. A store keeps its own lock, so every method takes `&self`. `LocalMemory` is a map that lives as long as the process. Save `LocalMemory::entries` and seed the next run with `LocalMemory::seeded` to keep memories between runs.

```rust
use guffbolt::Tool as _;
use guffbolt::tools::memory::{LocalMemory, Memory, MemoryRecall, Reach};
use guffbolt::tools::outbound::Allowlist;
use guffbolt::tools::{Local, Typed};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut local = Local::new("run-2", ".", Allowlist::Any)?;
    local.memory = LocalMemory::seeded(vec![Memory {
        key: "deploy:target".to_owned(),
        content: "staging first".to_owned(),
        category: "core".to_owned(),
        reach: Reach::Fleet,
    }]);
    let recall = Typed::new(MemoryRecall);

    let output = recall.call(&json!({ "query": "deploy" }), local.context()).await;
    assert_eq!(output.text, "deploy:target (core): staging first");
    Ok(())
}
```

### MCP servers

`tools::mcp` is a Model Context Protocol (MCP) client over the official `rmcp` SDK. `Server::stdio` starts a server through your `Env`, so it runs where your commands run. `Server::http` connects over streamable HTTP once your `Outbound` admits the URL. `Server::tools` offers each of the server's tools as `mcp__<server>__<tool>`. `ListMcpResources`, `ListMcpResourceTemplates` and `ReadMcpResource` list and read its resources.

```rust,no_run
use std::sync::Arc;

use guffbolt::tools::env::{Env, LocalEnv, Spawn};
use guffbolt::tools::mcp::{ReadMcpResource, Server};
use guffbolt::tools::{ContextTool, Typed};

async fn connect() -> Result<Vec<Box<ContextTool>>, Box<dyn std::error::Error>> {
    let env: Arc<dyn Env> = Arc::new(LocalEnv::new(".")?);
    let spawn = Spawn::program("npx").args(["-y", "@modelcontextprotocol/server-everything"]);
    let server = Arc::new(Server::stdio("everything", env, &spawn).await?);
    let mut tools: Vec<Box<ContextTool>> = Vec::new();
    for tool in server.tools().await? {
        tools.push(Box::new(tool));
    }
    tools.push(Box::new(Typed::new(ReadMcpResource::new(vec![server]))));
    Ok(tools)
}
```

### Deferred tools

A run with many tools, such as every tool of several MCP servers, can hold most of them unoffered with `Tools::hold`. `tool_search` searches their names and descriptions, and loads the best matches. Offer every loaded tool from the next model call, by asking `Deferred::is_loaded` in your `offered`.

## Dependencies

`guffbolt` depends on `tokio` for the time budget, the race against the stop and its children's channels, and on `tokio-util` for the stop's token. It uses `tracing` for its `debug` records and your spans, and `futures-util` for a model's stream of chunks and a run's set of children. `serde` and `serde_json` carry the transcript and the loop's saved progress, a call's arguments and a provider's replay record.

`bytes` carries an image a call read. Without `tools`, `guffbolt` links no model client, HTTP stack or secret store.

The `tools` feature adds `reqwest` for `LocalOutbound` and MCP over HTTP, `rmcp` for the MCP client, `schemars` for each tool's schema, `regex` for `grep` and `find`, `html2text` for `web_fetch`, `jiff` for `current_time`, `thiserror` for the vendored `apply_patch` parser, and Tokio's process, file and runtime features for `LocalEnv`.

A provider's record of a step rides it as an opaque `Replay`. `guffbolt` saves it and hands it back unopened to the provider that wrote it.

## License

MIT
