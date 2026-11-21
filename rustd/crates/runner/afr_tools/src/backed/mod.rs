//! The tools one run builds when it starts, each over a backend the run
//! holds, through `guffbolt`'s one builder.
//!
//! ```text
//!  Run ─┬─ executor source ─► LazySandboxEnv ─► with_env ─┬► shell, files, git, browser
//!       ├─ sessions ─────────────────────────► with_sessions ┴► exec_command, write_stdin
//!       ├─ guard, transport ─► Egressed ──────► with_outbound ─► http_request, web_fetch
//!       ├─ memory ────────────────────────────► with_memory ───► memory_*
//!       ├─ verbs, guard ─► Fenced ────────────► with_scheduler ─► cron_*, schedule
//!       │                                    └► with_channel ───► message
//!       ├─ guard, transport ─► Pushover ──────► with_notifier ──► notify
//!       └─ children ──────────────────────────► with_agents ────► guffbolt's subagent tools
//!                                    │
//!                     build(names the run offers) ─► RunTools
//! ```
//!
//! Each run has its own sandbox, guard, memory, fence and children, so these
//! tools are built per run and borrow the run for as long as it lasts (`'b`);
//! the catalog's hosted handlers serve every run and are lent the run per
//! call instead.

use std::fmt;

use af_egress::Transport;
use afr_executor::ExecutorSource;
use guffbolt::agents::Agents;
use guffbolt::tools::ToolsConfig;
use guffbolt::{Describe as _, Tools};

use crate::catalog::builds;
use crate::egress::Egressed;
use crate::error::{self, Result};
use crate::generic::{Generic, LazySandboxEnv};
use crate::pushover::Pushover;
use crate::run::Run;
use crate::runtime::RunTool;
use crate::verbs::Fenced;

/// The backends one run's tools are built over.
#[derive(Debug)]
pub struct Backends<'b, 'run> {
    run: &'b Run<'run>,
    /// The run's sandbox, each call waiting for it when the run's build has
    /// not finished.
    env: LazySandboxEnv<'b>,
    outbound: Egressed<'b, 'run>,
    fenced: Fenced<'b, 'run>,
    pushover: Pushover<'b, 'run>,
    /// The run's children; `None` builds no subagent tool.
    agents: Option<&'b Agents>,
}

impl<'b, 'run> Backends<'b, 'run> {
    /// The backends of `run`: the sandbox `executor` hands over, its memory
    /// and sessions, its verbs as its scheduler and channel, and its guard
    /// sending web requests and Pushover notifications through `transport`.
    /// No subagent tool is built until [`Backends::with_agents`] gives the
    /// run's children.
    #[must_use]
    pub const fn new(
        run: &'b Run<'run>,
        executor: &'b dyn ExecutorSource,
        transport: Option<&'b dyn Transport>,
    ) -> Self {
        Self {
            run,
            env: LazySandboxEnv::new(executor),
            outbound: Egressed::new(transport, run),
            fenced: Fenced::new(run),
            pushover: Pushover::new(Egressed::new(transport, run)),
            agents: None,
        }
    }

    /// The same backends, `guffbolt`'s subagent tools starting and steering
    /// the run's children `agents` keeps.
    #[must_use]
    pub const fn with_agents(mut self, agents: &'b Agents) -> Self {
        self.agents = Some(agents);
        self
    }

    /// The tools among `names` this run builds over its backends, in the
    /// order `names` gives them; a name the run does not build is passed
    /// over, for the catalog to host.
    ///
    /// # Errors
    /// A subagent tool named with no children given: the run is refused as
    /// naming a tool this runner cannot host.
    pub fn tools<S: AsRef<str>>(&self, names: &[S]) -> Result<RunTools<'_>> {
        let built: Vec<&str> = names
            .iter()
            .map(AsRef::as_ref)
            .filter(|name| builds(name))
            .collect();
        let config =
            ToolsConfig::new(&built).map_err(|refused| error::unhosted(refused.entry()))?;
        let run = self.run;
        let mut builder = Tools::builder()
            .with_env(&self.env)
            .with_sessions(&run.sessions)
            .with_image_input(run.image_input)
            .with_outbound(&self.outbound)
            .with_memory(&run.memory)
            .with_scheduler(&self.fenced)
            .with_channel(&self.fenced)
            .with_notifier(&self.pushover);
        // In the one repository the run holds; with several, the model names
        // one itself.
        if let [only] = run.checkouts.as_slice() {
            builder = builder.with_repository(only.name);
        }
        if let Some(agents) = self.agents {
            builder = builder.with_agents(agents);
        }
        let tools = builder
            .build(&config)
            .map_err(|missing| error::unhosted(missing.tool()))?;
        Ok(RunTools {
            tools: tools
                .into_offered()
                .into_iter()
                .map(Generic::built)
                .collect(),
        })
    }
}

/// The tools one run built over its backends.
pub struct RunTools<'b> {
    tools: Vec<Box<RunTool<'b>>>,
}

impl<'b> RunTools<'b> {
    /// The tool named `name`, when the run built one.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&RunTool<'b>> {
        self.tools
            .iter()
            .map(Box::as_ref)
            .find(|tool| tool.name() == name)
    }
}

impl fmt::Debug for RunTools<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.tools.iter().map(|tool| tool.name()).collect();
        f.debug_struct("RunTools").field("tools", &names).finish()
    }
}

#[cfg(test)]
mod tests;
