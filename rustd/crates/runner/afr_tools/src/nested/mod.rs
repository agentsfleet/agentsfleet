//! The nested-loop tools, as a policy names them: `delegate`, `spawn`,
//! `wait_agent`, `send_input`, `list_agents` and `interrupt_agent`.
//!
//! A child is another loop inside the run, so the loop runs these itself, as
//! tools of its own, and the catalog hosts no handler for them
//! (`docs/architecture/runner_execution.md` §"Tool catalog"). The catalog
//! still admits each as a `Supervisor` entry, so a policy naming one is
//! offered it. The argument types are public, because the loop parses a call
//! into them through [`parsed`](crate::parsed), and [`Nested`] names which
//! of the six a selection offers.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{
    DELEGATE, Entry, INTERRUPT_AGENT, LIST_AGENTS, SEND_INPUT, SPAWN, WAIT_AGENT,
};

/// The six, for a loop that offers none past the depth cap.
pub const NESTED: [&Entry; 6] = [
    &DELEGATE,
    &SPAWN,
    &WAIT_AGENT,
    &SEND_INPUT,
    &LIST_AGENTS,
    &INTERRUPT_AGENT,
];

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
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The id `spawn` answered with.
    pub child_id: u64,
}

/// `wait_agent`'s arguments.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
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

#[cfg(test)]
mod tests;
