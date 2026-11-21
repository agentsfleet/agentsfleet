//! The generic tools, behind the `tools` feature: what an agent does on a
//! machine and on the network, written once for every embedder.
//!
//! ```text
//!  Tools::builder() ─ with_env, with_sessions, with_outbound, with_memory,
//!        │            with_scheduler, with_channel, with_notifier, with_agents,
//!        │            with_tool
//!        ▼
//!  build(&ToolsConfig) ─► Tools: each named tool, holding its backend
//!        │                 (or MissingBackend naming the tool and backend)
//!        ▼
//!  Typed<Handler> ─► parse the arguments ─► run ─► ToolOutput the model reads
//!                 ▲
//!  embedder ──► Context { call_ref, run_id, wake } lent per call
//!
//!   env:       shell, git, file_*, apply_patch, image, ls, find, grep,
//!              browser_open, browser, screenshot
//!   env and sessions: exec_command, write_stdin
//!   outbound:  http_request, web_fetch
//!   memory:    memory_store, memory_recall, memory_list, memory_forget
//!   scheduler: cron_add, cron_list, cron_update, cron_remove, cron_run,
//!              cron_runs, schedule
//!   channel:   message
//!   notifier:  notify
//!   agents:    spawn_agent, send_input, followup_task, wait_agent,
//!              list_agents, close_agent, interrupt_agent
//!   none:      update_plan, ask_user, current_time, sleep
//! ```
//!
//! A backend is what a tool calls to do its work. Each tool takes its backend
//! when it is built, as pi's tools take their operations, so no registry is
//! looked up at call time: [`Env`] for commands and files, [`Sessions`] for
//! the processes kept across calls, [`Outbound`] for the network,
//! [`MemoryStore`] for memory, [`Scheduler`] for schedules, [`Channel`] for a
//! line to the conversation, [`Notifier`] for a notification and
//! [`crate::agents::Agents`] for a run's children. A third party writes a
//! tool whose `new` takes a backend of its own, publishes that backend's trait
//! beside it, and adds the tool with [`ToolsBuilder::with_tool`]. The same
//! tool runs on a developer's laptop over [`LocalEnv`], [`LocalOutbound`],
//! [`LocalMemory`], [`LocalScheduler`] and [`LocalChannel`], and inside an
//! embedder's sandbox behind its egress guard. Every tool is a [`Handler`]:
//! its arguments are a type whose JSON Schema the model is told, so what it is
//! told and what it reads cannot drift. [`Typed`] makes one a
//! [`crate::Tool`] lent a [`Context`], which carries only what one call is:
//! where it sits, the run it belongs to, and the wake.

#[macro_use]
mod backed;

pub mod agents;
pub mod ask;
pub mod browser;
pub mod builder;
pub mod clock;
pub mod code;
pub mod env;
pub mod files;
pub mod mcp;
pub mod memory;
pub mod message;
pub mod notify;
pub mod outbound;
pub mod plan;
pub mod process;
pub mod propose;
pub mod schedule;
pub mod search;
pub mod web;

#[cfg(test)]
pub(crate) mod fake;
mod local;
mod refusal;
mod schema;

use std::fmt;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};
use crate::transcript::CallRef;

pub use self::builder::{
    Backend, InvalidPattern, MissingBackend, ToolsBuilder, ToolsConfig, built_in,
};
pub use self::clock::Wake;
pub use self::env::{Env, LocalEnv};
pub use self::local::Local;
pub use self::memory::{LocalMemory, MemoryStore};
pub use self::message::{Channel, LocalChannel};
pub use self::notify::Notifier;
pub use self::outbound::{LocalOutbound, Outbound};
pub use self::process::sessions::Sessions;
pub use self::refusal::Refusal;
pub use self::schedule::{LocalScheduler, Scheduler};
pub use self::schema::{NoArguments, Schema};

/// A generic tool as a loop holds it: lent a [`Context`] per call, and
/// holding whatever it borrowed when it was built for as long as `'run`.
pub type ContextTool<'run> = dyn for<'c> Tool<Context<'c>> + 'run;

/// What a generic tool is lent for one call: where the call sits, the run it
/// belongs to, and what wakes a sleeping call.
///
/// The embedder builds one per call; every field is a borrow, so building
/// one costs nothing. A backend a tool calls is never here: the tool took it
/// when it was built.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// Where the call sits in its conversation; a backend that acts outside
    /// the process derives its idempotency key from it.
    pub call_ref: &'a CallRef,
    /// What names the run on the tools' log lines.
    pub run_id: &'a str,
    /// Woken when new input reaches the run, so `sleep` wakes early.
    pub wake: Option<&'a Wake>,
}

/// One tool's behaviour, given arguments that parsed.
///
/// The model is told the JSON Schema of [`Handler::Arguments`] and a call's
/// arguments parse into it, so an argument the type does not name refuses
/// the call (`#[serde(deny_unknown_fields)]`), the way Codex's tools parse
/// theirs. [`Typed`] is the one place a call is parsed. An implementation
/// writes `run` as an `async fn`; its future must be `Send`.
pub trait Handler: Send + Sync + fmt::Debug {
    /// The name the model calls it by.
    const NAME: &'static str;
    /// What the tool does, as the model reads it.
    const DESCRIPTION: &'static str;
    /// What one call carries.
    type Arguments: DeserializeOwned + JsonSchema + Send;

    /// Runs one call.
    fn run(
        &self,
        arguments: Self::Arguments,
        context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send;
}

/// A [`Handler`] as a loop holds it: its schema, built once.
#[derive(Debug)]
pub struct Typed<H> {
    handler: H,
    schema: Schema,
}

impl<H: Handler> Typed<H> {
    /// `handler`, its schema derived from its arguments' type.
    #[must_use]
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            schema: Schema::of::<H::Arguments>(H::DESCRIPTION),
        }
    }

    /// The handler it runs.
    #[must_use]
    pub const fn handler(&self) -> &H {
        &self.handler
    }
}

impl<H: Handler> Describe for Typed<H> {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(H::NAME)
    }
}

impl<'c, H: Handler> Tool<Context<'c>> for Typed<H> {
    fn call<'a>(&'a self, arguments: &'a Value, cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        Box::pin(async move {
            match parsed(arguments) {
                Ok(arguments) => self.handler.run(arguments, cx).await,
                Err(refused) => refused,
            }
        })
    }
}

/// `arguments` as `A`, or the refusal the model reads when they do not parse
/// as `A`'s schema: [`code::INVALID_ARGUMENTS`] and serde's sentence.
///
/// # Errors
/// The arguments break the schema: a field missing, mistyped, or not named.
pub fn parsed<A: DeserializeOwned>(arguments: &Value) -> Result<A, ToolOutput> {
    A::deserialize(arguments)
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))
}

#[cfg(test)]
#[path = "model_text_tests.rs"]
mod model_text_tests;
