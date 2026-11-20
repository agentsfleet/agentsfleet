//! Nested loops: `delegate`, `spawn`, `wait_agent`, `send_input`,
//! `list_agents` and `interrupt_agent`, tools the loop runs itself.
//!
//! A child is the run's loop over a task of the model's, with a selection
//! narrowed from its parent's: the same provider and key, sandbox, workspace,
//! memory, call counter and budget, and nothing of its own but its
//! conversation. `guffbolt::agents` keeps every child's state, inbox and
//! stop, refuses a child past the run's caps, and polls every child beside
//! the root. This module holds what is `agentsfleet`'s: the six tools'
//! published names, schemas and parsing, the caps' values, the child's loop
//! and its log (`child.rs`), and the ended children a recovery saves
//! (`recovery.rs`). A child at the depth cap is offered none of the six, and
//! the caps refuse a fifth running or a seventeenth started child
//! (`docs/architecture/runner_execution.md` §"Tool catalog").

use std::sync::LazyLock;

use afr_tools::nested::{Input, Named, Nested, Task, Wait};
use guffbolt::agents::Caps;
use guffbolt::tools::{NoArguments, Schema};
use guffbolt::{Describe, Tool, ToolFuture, ToolSpec};

pub(crate) use self::recovery::ChildRecovery;
use crate::router::Lent;

/// How deep a child may be: the root is 0, and a child this deep is offered
/// no nested tool.
const NESTED_DEPTH_MAX: u8 = 2;
/// How many children may run at once.
const CHILDREN_RUNNING_MAX: usize = 4;
/// How many children one run may start.
const CHILDREN_PER_RUN_MAX: u64 = 16;
/// The run's caps, as its registry enforces them and its recovery bounds
/// what it restores.
pub(crate) const CAPS: Caps = Caps {
    depth: NESTED_DEPTH_MAX,
    running: CHILDREN_RUNNING_MAX,
    started: CHILDREN_PER_RUN_MAX,
};

/// One of the six as a loop holds it: a tool of the loop's own, lent the
/// loop for each call, offered only where its selection names it.
#[derive(Debug)]
pub(crate) struct NestedTool {
    nested: Nested,
    schema: &'static Schema,
}

impl NestedTool {
    /// `nested`, under the schema it tells the model.
    pub(crate) fn new(nested: Nested) -> Self {
        Self {
            nested,
            schema: schema(nested),
        }
    }
}

impl Describe for NestedTool {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(self.nested.entry().name())
    }
}

impl<'call, 's, 'run> Tool<Lent<'call, 's, 'run>> for NestedTool {
    fn call<'a>(
        &'a self,
        arguments: &'a serde_json::Value,
        cx: Lent<'call, 's, 'run>,
    ) -> ToolFuture<'a>
    where
        Lent<'call, 's, 'run>: 'a,
    {
        // The nested tools need only the loop.
        let cx = cx.harness;
        // Each runs when the ledger polls it, inside the call's span, never
        // when the call is looked up.
        match self.nested {
            Nested::Delegate => Box::pin(delegate::run(cx, arguments)),
            Nested::Spawn => Box::pin(async move { spawn::run(cx, arguments) }),
            Nested::WaitAgent => Box::pin(wait::run(cx, arguments)),
            Nested::SendInput => Box::pin(async move { input::run(cx, arguments) }),
            Nested::ListAgents => Box::pin(async move { list::run(cx, arguments) }),
            Nested::InterruptAgent => Box::pin(async move { interrupt::run(cx, arguments) }),
        }
    }
}

/// What `nested` tells the model: its description over the schema of the
/// arguments it parses, built once.
fn schema(nested: Nested) -> &'static Schema {
    static DELEGATE: LazyLock<Schema> = LazyLock::new(|| Schema::of::<Task>(delegate::DESCRIPTION));
    static SPAWN: LazyLock<Schema> = LazyLock::new(|| Schema::of::<Task>(spawn::DESCRIPTION));
    static WAIT_AGENT: LazyLock<Schema> = LazyLock::new(|| Schema::of::<Wait>(wait::DESCRIPTION));
    static SEND_INPUT: LazyLock<Schema> = LazyLock::new(|| Schema::of::<Input>(input::DESCRIPTION));
    static LIST_AGENTS: LazyLock<Schema> =
        LazyLock::new(|| Schema::of::<NoArguments>(list::DESCRIPTION));
    static INTERRUPT_AGENT: LazyLock<Schema> =
        LazyLock::new(|| Schema::of::<Named>(interrupt::DESCRIPTION));
    match nested {
        Nested::Delegate => &DELEGATE,
        Nested::Spawn => &SPAWN,
        Nested::WaitAgent => &WAIT_AGENT,
        Nested::SendInput => &SEND_INPUT,
        Nested::ListAgents => &LIST_AGENTS,
        Nested::InterruptAgent => &INTERRUPT_AGENT,
    }
}

mod answer;
mod child;
mod delegate;
mod input;
mod interrupt;
mod list;
mod recovery;
mod spawn;
mod start;
mod wait;

#[cfg(test)]
mod fixture;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refusal_tests;

#[cfg(test)]
mod run_tests;

#[cfg(test)]
mod end_tests;

#[cfg(test)]
mod interrupt_tests;

#[cfg(test)]
mod answer_tests;

#[cfg(test)]
mod lifetime_tests;
