//! The nested-loop tools, as the model is told them: `delegate`, `spawn`,
//! `wait_agent`, `send_input`, `list_agents` and `interrupt_agent`.
//!
//! A child is another loop inside the run, so the loop itself runs these
//! beside the turn and the router never dispatches them
//! (`docs/architecture/runner_execution.md` §"Tool catalog"). The catalog
//! still hosts each as a `Supervisor` entry: a policy naming one is admitted,
//! its schema is offered, and a call that reached a handler by mistake reads a
//! refusal rather than running. The argument types are public, because the
//! loop parses a call into them through [`parsed`](crate::parsed), and
//! [`Nested`] names which of the six a call is.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{
    DELEGATE, Entry, INTERRUPT_AGENT, LIST_AGENTS, SEND_INPUT, SPAWN, WAIT_AGENT,
};
use crate::handler::{Handler, Typed};
use crate::runtime::{Tool, ToolContext, ToolErrorCode, ToolOutput};
use crate::schema::NoArguments;

/// The six, for a loop that offers none past the depth cap.
pub const NESTED: [&Entry; 6] = [
    &DELEGATE,
    &SPAWN,
    &WAIT_AGENT,
    &SEND_INPUT,
    &LIST_AGENTS,
    &INTERRUPT_AGENT,
];

/// What a call that reached a handler reads, since the loop runs these.
const RUN_BY_THE_LOOP: &str = "is run by the loop beside the turn, never by a handler";

/// One of the six, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nested {
    /// A child run to its answer.
    Delegate,
    /// A child started alongside.
    Spawn,
    /// A spawned child's end, or that it still runs.
    WaitAgent,
    /// A message for a spawned child's next turn.
    SendInput,
    /// Every child and its state.
    ListAgents,
    /// One spawned child ended.
    InterruptAgent,
}

impl Nested {
    /// Which of the six `name` is, when it is one.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        [
            Self::Delegate,
            Self::Spawn,
            Self::WaitAgent,
            Self::SendInput,
            Self::ListAgents,
            Self::InterruptAgent,
        ]
        .into_iter()
        .find(|nested| nested.entry().name() == name)
    }

    /// The published entry.
    #[must_use]
    pub const fn entry(self) -> &'static Entry {
        match self {
            Self::Delegate => &DELEGATE,
            Self::Spawn => &SPAWN,
            Self::WaitAgent => &WAIT_AGENT,
            Self::SendInput => &SEND_INPUT,
            Self::ListAgents => &LIST_AGENTS,
            Self::InterruptAgent => &INTERRUPT_AGENT,
        }
    }
}

/// The six, as a catalog hosts them: the schema each offers, and a handler
/// that refuses, since the loop runs them.
#[must_use]
pub fn tools() -> Vec<Box<dyn Tool>> {
    vec![
        Typed::boxed(Delegate),
        Typed::boxed(Spawn),
        Typed::boxed(WaitAgent),
        Typed::boxed(SendInput),
        Typed::boxed(ListAgents),
        Typed::boxed(InterruptAgent),
    ]
}

/// A child's task and the tools it may hold.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// What the child is to do, as its first message.
    pub task: String,
    /// The tools the child holds, each one this run holds; every one this
    /// run holds when absent.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// A child named by id.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The id `spawn` answered with.
    pub child_id: u64,
}

/// `wait_agent`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    /// The id `spawn` answered with.
    pub child_id: u64,
    /// How long to wait for the child to end, in milliseconds: 30000 when
    /// absent, at most 3600000; 0 only looks.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// `send_input`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// The id `spawn` answered with.
    pub child_id: u64,
    /// What the child reads on its next turn.
    pub message: String,
}

/// What any of the six answers when a handler, not the loop, was asked.
fn run_by_the_loop(entry: &Entry) -> ToolOutput {
    ToolOutput::failed(
        ToolErrorCode::NotOffered,
        &format!("{} {RUN_BY_THE_LOOP}", entry.name()),
    )
}

/// Runs a child to its answer.
#[derive(Debug)]
struct Delegate;

#[async_trait::async_trait]
impl Handler for Delegate {
    const ENTRY: &'static Entry = &DELEGATE;
    const DESCRIPTION: &'static str = "Hand a task to a child agent that shares this run's \
        sandbox, files, memory and budget, and wait for its answer. The child starts from the \
        task alone and may hold a subset of your tools.";
    type Arguments = Task;

    async fn run(&self, _arguments: Task, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

/// Starts a child that runs alongside.
#[derive(Debug)]
struct Spawn;

#[async_trait::async_trait]
impl Handler for Spawn {
    const ENTRY: &'static Entry = &SPAWN;
    const DESCRIPTION: &'static str = "Start a child agent on a task and keep working while it \
        runs; it shares this run's sandbox, files, memory and budget. Answers with a child_id \
        for wait_agent, send_input and interrupt_agent.";
    type Arguments = Task;

    async fn run(&self, _arguments: Task, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

/// A spawned child's answer, or that it still runs.
#[derive(Debug)]
struct WaitAgent;

#[async_trait::async_trait]
impl Handler for WaitAgent {
    const ENTRY: &'static Entry = &WAIT_AGENT;
    const DESCRIPTION: &'static str = "Wait for a spawned child to end and read its answer. \
        Answers its status, with the answer once it is done, and running when the timeout \
        passed first.";
    type Arguments = Wait;

    async fn run(&self, _arguments: Wait, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

/// A message for a spawned child's next turn.
#[derive(Debug)]
struct SendInput;

#[async_trait::async_trait]
impl Handler for SendInput {
    const ENTRY: &'static Entry = &SEND_INPUT;
    const DESCRIPTION: &'static str = "Send a running child a message it reads on its next \
        turn. Answers whether it was accepted; a child that has ended accepts none.";
    type Arguments = Input;

    async fn run(&self, _arguments: Input, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

/// Every child of the run and its state.
#[derive(Debug)]
struct ListAgents;

#[async_trait::async_trait]
impl Handler for ListAgents {
    const ENTRY: &'static Entry = &LIST_AGENTS;
    const DESCRIPTION: &'static str =
        "List every child agent of this run: its id, status, depth and how many calls it made.";
    type Arguments = NoArguments;

    async fn run(&self, _arguments: NoArguments, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

/// Ends one spawned child.
#[derive(Debug)]
struct InterruptAgent;

#[async_trait::async_trait]
impl Handler for InterruptAgent {
    const ENTRY: &'static Entry = &INTERRUPT_AGENT;
    const DESCRIPTION: &'static str =
        "Stop a spawned child where it is. Its open call ends interrupted, and so does it.";
    type Arguments = Named;

    async fn run(&self, _arguments: Named, _context: ToolContext<'_, '_>) -> ToolOutput {
        run_by_the_loop(Self::ENTRY)
    }
}

#[cfg(test)]
#[path = "nested/tests.rs"]
mod tests;
