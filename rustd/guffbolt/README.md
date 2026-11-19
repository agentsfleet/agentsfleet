# guffbolt

`guffbolt` is an agent harness for Rust. It is used by [agentsfleet](https://github.com/agentsfleet/agentsfleet), whose runner drives every agent run through it.

`guffbolt` runs an agent loop's turns in a fixed order. You supply the model, the conversation's input and window, and the tools.

`guffbolt` makes each model call, reads the turn, runs its calls and keeps the transcript. It decides when each step runs and when the loop ends, and how a tool call is looked up and refused. It tells your observer as each model call and tool call starts and ends.

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
drive(conversation, extension, state) ──► Ending: Answered | Failed | Stopped

every turn:  take input ─► before_model ─► window ─► model call ─► after_turn
no calls:    on_answer ─► remember ─► take input again; new input runs one more turn
calls:       run them ─► remember ─► after_group
```

A message that arrives while the model works joins the next model call. `guffbolt` never cuts a call short for it.

Before each model call, your `window` says what that call sends of the transcript. `guffbolt` makes the model call itself. It sends your `request` to your `Model` and reads the stream into a `Turn`.

`remember` puts the turn and its results into the `Transcript`. Your `scrub` runs first and hides what must not reach a model again.

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
    }
    assert_eq!(chat.notes.into_inner().unwrap(), ["Deploy the fix.\n\nUse staging."]);
}
```

`take_input` runs before every model call and once after an answer. A message sent while a tool runs reaches the next call. One sent while the model writes its answer runs one more turn, and reaches the model after that answer.

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
| `turn_started` | before each model call builds its request | log it, and open the span its stream is read in |
| `text` and `usage` | as each chunk of the stream arrives | stream the text live and meter the spend |
| `turn_ended` | once the turn is read, fails or is stopped | count it, and record what it spent |
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
| `budget` | once, before the first turn | bound the whole run's time |
| `before_model` | before every model call | read the task's new inputs, or end the run |
| `after_turn` | when a model turn arrives, before its tools run | refuse a turn, such as one cut at its output limit |
| `on_answer` | when the model answers without calling a tool | accept, end the run, or send the model back to work |
| `after_group` | after a turn's tool calls ran | check the goal after each batch of work |
| `budget_spent` | when the budget runs out | drop what the run can no longer finish |

`DriveState` carries the accepted answers and the completed turns from one run of a loop to the next. Save it with the transcript to resume a task later. A run of no special kind installs `Plain`.

## Tools

A tool implements `Describe`, which gives the model its name, description and parameters' JSON Schema. It also implements `Tool<Cx>`, which runs one call with the context `Cx` your loop lends it. `Cx` is yours, such as the state the calls share, or the loop itself.

`Tools` holds one loop's tools, offers them to the model in order and dispatches a call by name. A call to a name it holds no tool for is answered `[tool_not_offered]`. `Tools::hold` keeps a tool callable without offering it.

A tool answers with a `ToolOutput`: the text the model reads, an exit code, an `ErrorCode` and an `Image` to show the model. `guffbolt` answers a call from a turn cut at the output limit with `ToolOutput::cut`, `[output_limit_reached]`, and never runs it.

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
                Ending::Stopped => Status::Interrupted,
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
    }
}
```

`Children::beside` polls the root first, and every child whenever the root waits, all in the root's own future. No child is spawned onto the runtime, so a run that ends takes its children with it. `Children::end_all` drops what is left, and each child still running ends interrupted.

Implement `Raise` to say which loop a child runs, and to hear each one start and end. `Agents::start` narrows the child's tools through `Narrow`, and a child at the depth cap holds none that start children. Your tools that steer children call `Agents`, and answer with its shapes: `Spawned`, `State`, `Accepted` and `Listed`.

A `Pause` holds new children back, and is granted only while no child runs, unwinds or waits to start. Read `Agents::census` under it to save the children, and bring the ended ones back with `Agents::restored`.

## Media

`guffbolt::input` types a message as text plus image, audio, video or document parts. Your `MediaReader` turns each part into text the model reads. `Unread` names each part to the model until you read that kind.

## Dependencies

`guffbolt` depends on `tokio` for the time budget, the race against the stop and its children's channels, and on `tokio-util` for the stop's token. It uses `tracing` for its `debug` records and your spans, and `futures-util` for a model's stream of chunks and a run's set of children. `serde` and `serde_json` carry the transcript and the loop's saved progress, a call's arguments and a provider's replay record.

`bytes` carries an image a call read. `guffbolt` links no model client, HTTP stack or secret store.

A provider's record of a turn rides it as an opaque `Replay`. `guffbolt` saves it and hands it back unopened to the provider that wrote it.

## License

MIT
