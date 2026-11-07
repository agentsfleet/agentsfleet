# guffbolt

`guffbolt` is the agent harness behind [agentsfleet](https://github.com/agentsfleet/agentsfleet). The agentsfleet runner drives every agent run through it, chats and durable tasks alike.

`guffbolt` runs an agent loop's turns in a fixed order. You supply the conversation, the model call and the tools. `guffbolt` decides when each step runs and when the loop ends.

## Prerequisites

- **Rust** with edition 2024, at or above the `rust-version` in `Cargo.toml`.
- **The crate.** It is not on crates.io yet, so depend on it from the agentsfleet repository. Crates inside agentsfleet write `guffbolt.workspace = true`.
- **`Send` futures.** Each async method you write must return a `Send` future, because the traits require it.
- **Tokio, for a time budget.** `Budget::Within` uses Tokio's timer, so that loop must run inside a Tokio runtime with time enabled.

```toml
[dependencies]
guffbolt = { git = "https://github.com/agentsfleet/agentsfleet" }
```

## How a loop runs

```text
drive(conversation, extension) ──► Ending: Answered | Failed | Stopped

every turn:  take input ─► before_model ─► prepare ─► sample ─► after_turn
no calls:    on_answer ─► take input again; new input runs one more turn
calls:       run them ─► remember ─► after_group
```

A message that arrives while the model works joins the next model call. `guffbolt` never cuts a call short for it.

## A plain chat

Implement `ModelTurn` for what one model call returns, and `Conversation` for your chat. Then call `drive` with `Plain`, which adds nothing to the loop.

```rust
use std::sync::mpsc::{Receiver, channel};

use guffbolt::{Conversation, Ending, ModelTurn, Plain, Prepared, Sampled, drive};

/// What one model call returned.
struct Reply {
    text: String,
    calls: Vec<String>,
}

impl ModelTurn for Reply {
    fn text(&self) -> &str {
        &self.text
    }
    fn has_calls(&self) -> bool {
        !self.calls.is_empty()
    }
    fn cut(&self) -> bool {
        false
    }
}

/// A chat: its messages so far, and the ones people send while it runs.
struct Chat {
    messages: Vec<String>,
    inbox: Receiver<String>,
}

impl Conversation for Chat {
    type Turn = Reply;
    type Results = Vec<String>;
    type Failure = String;

    async fn take_input(&mut self) -> bool {
        let before = self.messages.len();
        self.messages.extend(self.inbox.try_iter());
        self.messages.len() > before
    }

    async fn prepare(&mut self) -> Prepared<String> {
        // Trim or summarize the messages to fit the model's window here.
        Prepared::Ready
    }

    fn begin_group(&mut self) {}

    async fn sample(&mut self, _number: u64) -> Sampled<Reply, String> {
        // Call your model with the messages here.
        let last = self.messages.last().cloned().unwrap_or_default();
        Sampled::Turn(Reply { text: format!("You said: {last}"), calls: Vec::new() })
    }

    async fn run_calls(&mut self, turn: &Reply) -> Vec<String> {
        // Run each tool the model asked for, and keep what each one said.
        turn.calls.iter().map(|call| format!("{call}: done")).collect()
    }

    fn remember(&mut self, turn: Reply, results: Vec<String>) {
        self.messages.push(turn.text);
        self.messages.extend(results);
    }

    fn push_user(&mut self, text: String) {
        self.messages.push(text);
    }

    fn stopped(&self) -> bool {
        false
    }

    fn silent(&self, _cut: bool) -> String {
        "the model answered nothing".to_owned()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (send, inbox) = channel();
    let mut chat = Chat { messages: vec!["Deploy the fix.".to_owned()], inbox };
    send.send("Use staging.".to_owned()).unwrap();

    match drive(&mut chat, &mut Plain).await {
        // The model read both messages before it answered.
        Ending::Answered(text) => assert_eq!(text, "You said: Use staging."),
        Ending::Failed(why) => panic!("{why}"),
        Ending::Stopped => panic!("nothing stopped this loop"),
    }
}
```

`take_input` runs before every model call and once after an answer. A message sent while a tool runs reaches the next call. One sent while the model writes its answer runs one more turn, and reaches the model after that answer.

## A durable task

A durable task adds its rules to the same loop through an `Extension`. Every hook defaults to adding nothing, so a task overrides only the hooks it needs.

```rust
use std::time::Duration;

use guffbolt::{Answer, Budget, Conversation, Ending, Extension, drive};

/// A task: at most a minute of work, and no answer until the goal is met.
struct Task {
    goal: &'static str,
    budget: Duration,
}

impl<C: Conversation> Extension<C> for Task {
    fn budget(&self, _conversation: &C) -> Budget {
        Budget::Within(self.budget)
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
async fn run_task<C: Conversation>(conversation: &mut C) -> Ending<C::Failure> {
    let mut task = Task { goal: "deployed", budget: Duration::from_secs(60) };
    drive(conversation, &mut task).await
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

The agentsfleet runner installs its own `Task` extension on every durable task. It reads the task's inputs before each model call, checks the goal after each batch of tool calls, and bounds a task that acts by its execution slice. Every other run installs `Plain`.

## Media

`guffbolt::input` types a message as text plus image, audio, video or document parts. Your `MediaReader` turns each part into text the model reads. `Unread` names each part to the model until you read that kind.

## Dependencies

`guffbolt` depends on `tokio`, for the time budget, and `tracing`, for the loop's `debug` records. It links no model client, HTTP stack or secret store.

## License

MIT
