//! [`ToolsBuilder`]: the one way a loop's tools are built. Each tool takes
//! its backend when it is built, and a startup [`ToolsConfig`] names the
//! tools the run offers.
//!
//! ```text
//!  Tools::builder()
//!    .with_env(&env)            shell, files, git, browser
//!    .with_outbound(&outbound)  http_request, web_fetch
//!    .with_memory(&memory)      memory_*
//!    .with_scheduler(&jobs)     cron_*, schedule
//!    .with_channel(&channel)    message
//!    .with_notifier(&vendor)    notify
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
mod seamed;

use std::fmt;

use crate::tool::{Tool, Tools};
use crate::tools::{
    Channel, Context, ContextTool, Env, MemoryStore, Notifier, Outbound, Scheduler,
};

use self::config::Pattern;
pub use self::config::{InvalidPattern, ToolsConfig};

/// What a built-in tool calls to do its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Where commands run and files live: [`Env`].
    Env,
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
}

impl Backend {
    /// Its name, as a refusal reads it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Outbound => "outbound",
            Self::Memory => "memory",
            Self::Scheduler => "scheduler",
            Self::Channel => "channel",
            Self::Notifier => "notifier",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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

/// The backends a builder was given, and the tools added already built.
#[derive(Debug, Default)]
pub struct ToolsBuilder<'run> {
    env: Option<&'run dyn Env>,
    outbound: Option<&'run dyn Outbound>,
    memory: Option<&'run dyn MemoryStore>,
    scheduler: Option<&'run dyn Scheduler>,
    channel: Option<&'run dyn Channel>,
    notifier: Option<&'run dyn Notifier>,
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
    pub fn with_env(mut self, env: &'run dyn Env) -> Self {
        self.env = Some(env);
        self
    }

    /// The builder, its web tools sending through `outbound`.
    #[must_use]
    pub fn with_outbound(mut self, outbound: &'run dyn Outbound) -> Self {
        self.outbound = Some(outbound);
        self
    }

    /// The builder, its memory tools over `memory`.
    #[must_use]
    pub fn with_memory(mut self, memory: &'run dyn MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// The builder, its schedule tools over `scheduler`.
    #[must_use]
    pub fn with_scheduler(mut self, scheduler: &'run dyn Scheduler) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// The builder, `message` posting through `channel`.
    #[must_use]
    pub fn with_channel(mut self, channel: &'run dyn Channel) -> Self {
        self.channel = Some(channel);
        self
    }

    /// The builder, `notify` sending through `notifier`.
    #[must_use]
    pub fn with_notifier(mut self, notifier: &'run dyn Notifier) -> Self {
        self.notifier = Some(notifier);
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
    pub fn build(self, config: &ToolsConfig) -> Result<Tools<ContextTool<'run>>, MissingBackend> {
        let given = Given::from(&self);
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

/// The backends a builder was given, copied out of it.
#[derive(Clone, Copy)]
struct Given<'run> {
    env: Option<&'run dyn Env>,
    outbound: Option<&'run dyn Outbound>,
    memory: Option<&'run dyn MemoryStore>,
    scheduler: Option<&'run dyn Scheduler>,
    channel: Option<&'run dyn Channel>,
    notifier: Option<&'run dyn Notifier>,
}

impl<'run> From<&ToolsBuilder<'run>> for Given<'run> {
    fn from(builder: &ToolsBuilder<'run>) -> Self {
        Self {
            env: builder.env,
            outbound: builder.outbound,
            memory: builder.memory,
            scheduler: builder.scheduler,
            channel: builder.channel,
            notifier: builder.notifier,
        }
    }
}

impl<'run> Given<'run> {
    /// Appends to `offered` each built-in tool `pattern` matches that it does
    /// not hold yet: a name whose backend is missing refuses, and a family
    /// pattern passes over it.
    fn built_ins(
        self,
        pattern: &Pattern,
        offered: &mut Vec<Box<ContextTool<'run>>>,
    ) -> Result<(), MissingBackend> {
        for family in &catalog::FAMILIES {
            for &name in family.names {
                if !pattern.matches(name) || holds(offered, name) {
                    continue;
                }
                match (self.tool(name), family.needs) {
                    (Some(tool), _) => offered.push(tool),
                    (None, Some(backend)) if matches!(pattern, Pattern::Name(_)) => {
                        return Err(MissingBackend {
                            tool: name.to_owned(),
                            backend,
                        });
                    }
                    (None, _) => {}
                }
            }
        }
        Ok(())
    }

    /// The built-in tool named `name`, over the backend it needs; `None`
    /// when that backend was not given.
    fn tool(self, name: &str) -> Option<Box<ContextTool<'run>>> {
        catalog::plain_tool(name)
            .or_else(|| catalog::env_tool(name, self.env?))
            .or_else(|| catalog::outbound_tool(name, self.outbound?))
            .or_else(|| catalog::memory_tool(name, self.memory?))
            .or_else(|| catalog::schedule_tool(name, self.scheduler?))
            .or_else(|| catalog::channel_tool(name, self.channel?))
            .or_else(|| catalog::notifier_tool(name, self.notifier?))
    }
}

#[cfg(test)]
mod tests;
