//! The subagent tools, named as models are trained to call them:
//! `spawn_agent`, `send_input`, `followup_task`, `wait_agent`,
//! `list_agents`, `close_agent` and `interrupt_agent`.
//!
//! Each is built over a run's [`Agents`] with
//! [`crate::tools::ToolsBuilder::with_agents`], parses its arguments, and
//! answers with an order the loop runs in the calling loop's place
//! ([`crate::agents`]). A child is another loop of the run over its own
//! transcript: it starts from its parent's history, all of it, none of it or
//! its last turns, as `fork_turns` says, holds its parent's tools or the
//! subset named, and goes idle when its turn ends, keeping its transcript for
//! a follow-up task until it is closed or the loop ends.

use std::future::{Ready, ready};
use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::agents::{
    Action, Agents, CLOSE_AGENT, FOLLOWUP_TASK, Fork, INTERRUPT_AGENT, LIST_AGENTS, Order,
    SEND_INPUT, SPAWN_AGENT, Spawn, WAIT_AGENT,
};
use crate::tool::ToolOutput;
use crate::tools::{Context, Handler, NoArguments, code};

/// How long a wait lasts when the model names no timeout.
const WAIT_DEFAULT: Duration = Duration::from_secs(30);
/// The longest wait: an hour. Zero is allowed, to only look.
const WAIT_MAX: Duration = Duration::from_hours(1);
/// What a `fork_turns` the tool cannot read answers.
const FORK_TURNS: &str = "fork_turns must be `none`, `all` or a number of turns";

/// `spawn_agent`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpawnArguments {
    /// The child's first task, as the message it reads.
    message: String,
    /// The tools the child holds, each one you hold; every one you hold when
    /// absent.
    #[serde(default)]
    tools: Option<Vec<String>>,
    /// How much of your history the child starts from, before its task:
    /// `all` (the default) for every turn, `none` for its task alone, or a
    /// number for your last that many turns.
    #[serde(default)]
    fork_turns: Option<String>,
}

/// A child named by the id `spawn_agent` answered, and a message for it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Steer {
    /// The `agent_id` `spawn_agent` answered with.
    target: u64,
    /// What the child reads.
    message: String,
}

/// `wait_agent`'s arguments.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    /// The `agent_id` `spawn_agent` answered with.
    target: u64,
    /// How long to wait for the child's turn to end, in milliseconds: 30000
    /// when absent, at most 3600000; 0 only looks.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// A child named by the id `spawn_agent` answered.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// The `agent_id` `spawn_agent` answered with.
    target: u64,
}

/// Declares one subagent tool: it holds a copy of the run's [`Agents`].
macro_rules! agent_tool {
    ($(#[$doc:meta])* $tool:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $tool {
            agents: Agents,
        }

        impl $tool {
            /// The tool, over the children `agents` keeps.
            #[must_use]
            pub fn new(agents: &Agents) -> Self {
                Self {
                    agents: agents.clone(),
                }
            }

            /// The answer that hands the loop `action`.
            fn order(&self, action: Action) -> Ready<ToolOutput> {
                ready(ToolOutput::ordering(Order::new(self.agents.clone(), action)))
            }
        }
    };
}

agent_tool! {
    /// Starts a child.
    SpawnAgent
}

agent_tool! {
    /// Queues a message for a child.
    SendInput
}

agent_tool! {
    /// Gives a child another task.
    FollowupTask
}

agent_tool! {
    /// Waits for a child's turn to end.
    WaitAgent
}

agent_tool! {
    /// Lists the run's children.
    ListAgents
}

agent_tool! {
    /// Closes a child.
    CloseAgent
}

agent_tool! {
    /// Interrupts a child's turn.
    InterruptAgent
}

impl Handler for SpawnAgent {
    const NAME: &'static str = SPAWN_AGENT;
    const DESCRIPTION: &'static str = "Start a child agent on a well-scoped task. It runs \
        alongside you over its own history, with your tools or the subset you name, and reaches \
        what your tools reach. Answers with an agent_id for send_input, followup_task, \
        wait_agent, close_agent and interrupt_agent. It starts from all of your history unless \
        fork_turns says none, or a number of your last turns; with none it knows only its task \
        and may lack what it needs. When its turn ends it stays open with its history until you \
        close it.";
    type Arguments = SpawnArguments;

    fn run(
        &self,
        arguments: SpawnArguments,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let fork = arguments
            .fork_turns
            .as_deref()
            .map_or(Some(Fork::All), Fork::parse);
        let Some(fork) = fork else {
            return ready(ToolOutput::failed(code::INVALID_ARGUMENTS, FORK_TURNS));
        };
        self.order(Action::Spawn(Spawn {
            message: arguments.message,
            tools: arguments.tools,
            fork,
        }))
    }
}

impl Handler for SendInput {
    const NAME: &'static str = SEND_INPUT;
    const DESCRIPTION: &'static str = "Queue a message for a child agent: a running child \
        reads it on its next step, an idle one with its next task. Answers whether it was \
        accepted; a closed child accepts none. To give an idle child work, use followup_task.";
    type Arguments = Steer;

    fn run(
        &self,
        arguments: Steer,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        self.order(Action::Send {
            target: arguments.target,
            message: arguments.message,
        })
    }
}

impl Handler for FollowupTask {
    const NAME: &'static str = FOLLOWUP_TASK;
    const DESCRIPTION: &'static str = "Give a child agent another task. An idle child runs \
        again with its whole history; a running one reads the task on its next step. Answers \
        whether it was accepted; a closed child accepts none.";
    type Arguments = Steer;

    fn run(
        &self,
        arguments: Steer,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        self.order(Action::FollowUp {
            target: arguments.target,
            message: arguments.message,
        })
    }
}

impl Handler for WaitAgent {
    const NAME: &'static str = WAIT_AGENT;
    const DESCRIPTION: &'static str = "Wait for a child agent's turn to end and read its \
        answer. Answers its status, with its answer once done or its detail once failed, and \
        running when the timeout passed first.";
    type Arguments = Wait;

    fn run(
        &self,
        arguments: Wait,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        // The model's `timeout_ms` is the tool call's edge; it becomes a span
        // here.
        let timeout = arguments
            .timeout_ms
            .map_or(WAIT_DEFAULT, Duration::from_millis)
            .min(WAIT_MAX);
        self.order(Action::Wait {
            target: arguments.target,
            timeout,
        })
    }
}

impl Handler for ListAgents {
    const NAME: &'static str = LIST_AGENTS;
    const DESCRIPTION: &'static str = "List every child agent of this run: its agent_id, \
        status, depth and how many calls it made.";
    type Arguments = NoArguments;

    fn run(
        &self,
        _arguments: NoArguments,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        self.order(Action::List)
    }
}

impl Handler for CloseAgent {
    const NAME: &'static str = CLOSE_AGENT;
    const DESCRIPTION: &'static str = "Close a child agent, and every child it started, once \
        it is no longer needed. A closed child keeps no history and frees its place, so \
        another may start. Answers its status before it closed.";
    type Arguments = Target;

    fn run(
        &self,
        arguments: Target,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        self.order(Action::Close {
            target: arguments.target,
        })
    }
}

impl Handler for InterruptAgent {
    const NAME: &'static str = INTERRUPT_AGENT;
    const DESCRIPTION: &'static str = "Interrupt a child agent's current turn, if it runs one, \
        and answer its status before. The child stays open for messages and follow-up tasks.";
    type Arguments = Target;

    fn run(
        &self,
        arguments: Target,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        self.order(Action::Interrupt {
            target: arguments.target,
        })
    }
}

#[cfg(test)]
mod tests;
