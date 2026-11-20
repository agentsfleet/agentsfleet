//! [`ToolsBuilder`]: the one way a loop's tools are built. Each tool takes
//! its backend when it is built, and a startup [`ToolsConfig`] names the
//! tools the run offers.
//!
//! ```text
//!  Tools::builder()
//!    .with_env(&env)            shell, files, git, browser
//!    .with_sessions(&sessions)  exec_command, write_stdin (with the env)
//!    .with_outbound(&outbound)  http_request, web_fetch
//!    .with_memory(&memory)      memory_*
//!    .with_scheduler(&jobs)     cron_*, schedule
//!    .with_channel(&channel)    message
//!    .with_notifier(&vendor)    notify
//!    .with_agents(&agents)      spawn_agent, send_input, followup_task, ...
//!    .with_tool(tool)           a tool built with a backend of its own
//!    .build(&config) ─► Tools, or MissingBackend { tool, backend }
//! ```
//!
//! A builder borrows its backends for `'run`, the life of the run that built
//! it, and every tool it builds holds what it borrowed: no registry is looked
//! up at call time, and a tool the config names whose backend was never
//! given stops the run before its first step.

mod catalog;
mod config;

use std::fmt;

use crate::agents::Agents;
use crate::tool::{Tool, Tools};
use crate::tools::{
    Channel, Context, ContextTool, Env, MemoryStore, Notifier, Outbound, Scheduler, Sessions,
};

use self::config::Pattern;
pub use self::config::{InvalidPattern, ToolsConfig};

/// What a built-in tool calls to do its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Where commands run and files live: [`Env`].
    Env,
    /// The processes a run keeps open across calls: [`Sessions`].
    Sessions,
    /// Where requests leave for the network: [`Outbound`].
    Outbound,
    /// Where memories are kept: [`MemoryStore`].
    Memory,
    /// Where jobs are kept and fired: [`Scheduler`].
    Scheduler,
    /// Where a line to the conversation goes: [`Channel`].
    Channel,
    /// Where a notification goes: [`Notifier`].
    Notifier,
    /// A run's children: [`Agents`].
    Agents,
}

impl Backend {
    /// Its name, as a refusal reads it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Sessions => "sessions",
            Self::Outbound => "outbound",
            Self::Memory => "memory",
            Self::Scheduler => "scheduler",
            Self::Channel => "channel",
            Self::Notifier => "notifier",
            Self::Agents => "agents",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What building a run's tools answers.
pub type Result<T, E = MissingBackend> = core::result::Result<T, E>;

/// A tool the config names whose backend the builder was never given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingBackend {
    tool: String,
    backend: Backend,
}

impl MissingBackend {
    /// The tool the config named.
    #[must_use]
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// The backend it needs.
    #[must_use]
    pub const fn backend(&self) -> Backend {
        self.backend
    }
}

impl fmt::Display for MissingBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} needs a {} backend, and the builder was given none",
            self.tool, self.backend
        )
    }
}

impl std::error::Error for MissingBackend {}

/// A tool the builder builds itself, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltIn {
    /// The name the model calls it by.
    pub name: &'static str,
    /// The backends it is built over; none for a tool that takes none.
    pub needs: &'static [Backend],
}

/// The built-in tool named `name`, when the builder builds one: what an
/// embedder that publishes its own list of tools asks before it refuses a
/// name it does not know.
#[must_use]
pub fn built_in(name: &str) -> Option<BuiltIn> {
    catalog::TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .copied()
}

/// The backends a builder was given, and the tools added already built.
#[derive(Debug, Default)]
pub struct ToolsBuilder<'run> {
    given: Given<'run>,
    added: Vec<Box<ContextTool<'run>>>,
}

impl<'run> Tools<ContextTool<'run>> {
    /// A builder with no backend and no tool.
    #[must_use]
    pub fn builder() -> ToolsBuilder<'run> {
        ToolsBuilder::default()
    }
}

impl<'run> ToolsBuilder<'run> {
    /// The builder, its environment tools running in `env`.
    #[must_use]
    pub const fn with_env(mut self, env: &'run dyn Env) -> Self {
        self.given.env = Some(env);
        self
    }

    /// The builder, `exec_command` and `write_stdin` keeping their
    /// processes in `sessions`, which the embedder closes when the run ends.
    #[must_use]
    pub const fn with_sessions(mut self, sessions: &'run Sessions) -> Self {
        self.given.sessions = Some(sessions);
        self
    }

    /// The builder, `git` running in `repository`, relative to the
    /// environment's root: the one repository the embedder checked out.
    /// Without it, `git` runs at the root.
    #[must_use]
    pub const fn with_repository(mut self, repository: &'run str) -> Self {
        self.given.repository = Some(repository);
        self
    }

    /// The builder, `image` showing the model what it reads when the model's
    /// wire takes an image with a call's result, and refusing before any read
    /// when it does not. Without it, `image` refuses.
    #[must_use]
    pub const fn with_image_input(mut self, image_input: bool) -> Self {
        self.given.image_input = image_input;
        self
    }

    /// The builder, its web tools sending through `outbound`.
    #[must_use]
    pub const fn with_outbound(mut self, outbound: &'run dyn Outbound) -> Self {
        self.given.outbound = Some(outbound);
        self
    }

    /// The builder, its memory tools over `memory`.
    #[must_use]
    pub const fn with_memory(mut self, memory: &'run dyn MemoryStore) -> Self {
        self.given.memory = Some(memory);
        self
    }

    /// The builder, its schedule tools over `scheduler`.
    #[must_use]
    pub const fn with_scheduler(mut self, scheduler: &'run dyn Scheduler) -> Self {
        self.given.scheduler = Some(scheduler);
        self
    }

    /// The builder, `message` posting through `channel`.
    #[must_use]
    pub const fn with_channel(mut self, channel: &'run dyn Channel) -> Self {
        self.given.channel = Some(channel);
        self
    }

    /// The builder, `notify` sending through `notifier`.
    #[must_use]
    pub const fn with_notifier(mut self, notifier: &'run dyn Notifier) -> Self {
        self.given.notifier = Some(notifier);
        self
    }

    /// The builder, its subagent tools starting and steering the children
    /// `agents` keeps, under its caps: what turns subagents on.
    #[must_use]
    pub const fn with_agents(mut self, agents: &'run Agents) -> Self {
        self.given.agents = Some(agents);
        self
    }

    /// The builder, also holding `tool`, built already with whatever backend
    /// it takes. A config entry matching its name offers it, ahead of a
    /// built-in tool of the same name.
    #[must_use]
    pub fn with_tool<T>(mut self, tool: T) -> Self
    where
        T: for<'c> Tool<Context<'c>> + 'run,
    {
        self.added.push(Box::new(tool));
        self
    }

    /// The tools `config` names, in its order, each holding its backend.
    ///
    /// # Errors
    /// A name in `config` is a built-in tool whose backend this builder was
    /// not given; nothing is built.
    pub fn build(self, config: &ToolsConfig) -> Result<Tools<ContextTool<'run>>> {
        let given = self.given;
        let mut added: Vec<Option<Box<ContextTool<'run>>>> =
            self.added.into_iter().map(Some).collect();
        let mut offered: Vec<Box<ContextTool<'run>>> = Vec::new();
        for pattern in config.patterns() {
            for slot in &mut added {
                let named = slot.as_ref().is_some_and(|tool| {
                    pattern.matches(tool.name()) && !holds(&offered, tool.name())
                });
                if named {
                    offered.extend(slot.take());
                }
            }
            given.built_ins(pattern, &mut offered)?;
        }
        Ok(Tools::new(offered))
    }
}

/// Whether `offered` already holds a tool named `name`.
fn holds(offered: &[Box<ContextTool<'_>>], name: &str) -> bool {
    offered.iter().any(|held| held.name() == name)
}

/// The backends a builder was given.
#[derive(Debug, Default, Clone, Copy)]
struct Given<'run> {
    env: Option<&'run dyn Env>,
    sessions: Option<&'run Sessions>,
    repository: Option<&'run str>,
    image_input: bool,
    outbound: Option<&'run dyn Outbound>,
    memory: Option<&'run dyn MemoryStore>,
    scheduler: Option<&'run dyn Scheduler>,
    channel: Option<&'run dyn Channel>,
    notifier: Option<&'run dyn Notifier>,
    agents: Option<&'run Agents>,
}

impl<'run> Given<'run> {
    /// Appends to `offered` each built-in tool `pattern` matches that it does
    /// not hold yet: a name whose backend is missing refuses, and a family
    /// pattern passes over it.
    fn built_ins(self, pattern: &Pattern, offered: &mut Vec<Box<ContextTool<'run>>>) -> Result<()> {
        for built_in in &catalog::TOOLS {
            let name = built_in.name;
            if !pattern.matches(name) || holds(offered, name) {
                continue;
            }
            match built_in.needs.iter().find(|needed| !self.gives(**needed)) {
                None => offered.extend(catalog::tool(self, name)),
                Some(&backend) if matches!(pattern, Pattern::Name(_)) => {
                    return Err(MissingBackend {
                        tool: name.to_owned(),
                        backend,
                    });
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// Whether the builder was given `backend`.
    const fn gives(self, backend: Backend) -> bool {
        match backend {
            Backend::Env => self.env.is_some(),
            Backend::Sessions => self.sessions.is_some(),
            Backend::Outbound => self.outbound.is_some(),
            Backend::Memory => self.memory.is_some(),
            Backend::Scheduler => self.scheduler.is_some(),
            Backend::Channel => self.channel.is_some(),
            Backend::Notifier => self.notifier.is_some(),
            Backend::Agents => self.agents.is_some(),
        }
    }
}

#[cfg(test)]
mod tests;
