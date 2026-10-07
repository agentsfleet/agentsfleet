//! The nested-loop tools, as the model is told them: `delegate`, `spawn`,
//! `wait_agent`, `send_input`, `list_agents` and `interrupt_agent`.
//!
//! A child is another loop inside the run, so the loop itself runs these
//! beside the turn and the router never dispatches them
//! (`docs/architecture/runner_execution.md` §"Tool catalog"). The catalog
//! still hosts each as a `Supervisor` entry: a policy naming one is admitted,
//! its schema is offered, and a call that reached a handler by mistake reads a
//! refusal rather than running. The argument types are public, because the
//! loop parses a call into them through [`parsed`](crate::parsed).

use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{
    DELEGATE, Entry, INTERRUPT_AGENT, LIST_AGENTS, SEND_INPUT, SPAWN, WAIT_AGENT,
};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};
use crate::stub::NoArguments;

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

/// Whether `name` is one of the six.
#[must_use]
pub fn is_nested(name: &str) -> bool {
    NESTED.iter().any(|entry| entry.name() == name)
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
pub(crate) struct Delegate;

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
pub(crate) struct Spawn;

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
pub(crate) struct WaitAgent;

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
pub(crate) struct SendInput;

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
pub(crate) struct ListAgents;

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
pub(crate) struct InterruptAgent;

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
