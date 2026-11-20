//! The tools one run builds when it starts, each over a backend the run
//! holds, through `guffbolt`'s one builder.
//!
//! ```text
//!  Run ─┬─ memory ─────────────────► with_memory    ─► memory_*
//!       ├─ verbs, guard ─► Fenced ─► with_scheduler ─► cron_*, schedule
//!       │                         └► with_channel   ─► message
//!       └─ guard, transport ─► Pushover ─► with_notifier ─► notify
//!                                    │
//!                     build(names the run offers) ─► RunTools
//! ```
//!
//! Each run has its own memory, guard and fence, so these tools are built per
//! run and borrow the run for as long as it lasts (`'b`); the catalog's
//! hosted handlers serve every run and are lent the run per call instead.

use std::fmt;

use af_egress::Transport;
use guffbolt::tools::ToolsConfig;
use guffbolt::{Describe as _, Tools};

use crate::catalog::BACKED;
use crate::egress::Egressed;
use crate::error::{self, Result};
use crate::generic::Generic;
use crate::pushover::Pushover;
use crate::run::Run;
use crate::runtime::RunTool;
use crate::verbs::Fenced;

/// The backends one run's tools are built over.
#[derive(Debug)]
pub struct Backends<'b, 'run> {
    run: &'b Run<'run>,
    fenced: Fenced<'b, 'run>,
    pushover: Pushover<'b, 'run>,
}

impl<'b, 'run> Backends<'b, 'run> {
    /// The backends of `run`: its memory, its verbs as its scheduler and
    /// channel, and Pushover sending through its guard and `transport`.
    #[must_use]
    pub fn new(run: &'b Run<'run>, transport: Option<&'b dyn Transport>) -> Self {
        Self {
            run,
            fenced: Fenced::new(run),
            pushover: Pushover::new(Egressed::new(transport, run)),
        }
    }

    /// The tools among `names` this run builds over its backends, in the
    /// order `names` gives them; a name the run does not build is passed
    /// over, for the catalog to host.
    ///
    /// # Errors
    /// None in practice: every backend a built tool needs is given, and a
    /// published name is never a pattern. Either would refuse the run as
    /// naming a tool this runner cannot host.
    pub fn tools<S: AsRef<str>>(&self, names: &[S]) -> Result<RunTools<'_>> {
        let backed: Vec<&str> = names
            .iter()
            .map(AsRef::as_ref)
            .filter(|name| BACKED.iter().any(|entry| entry.name() == *name))
            .collect();
        let config =
            ToolsConfig::new(&backed).map_err(|refused| error::unhosted(refused.entry()))?;
        let built = Tools::builder()
            .with_memory(&self.run.memory)
            .with_scheduler(&self.fenced)
            .with_channel(&self.fenced)
            .with_notifier(&self.pushover)
            .build(&config)
            .map_err(|missing| error::unhosted(missing.tool()))?;
        Ok(RunTools {
            tools: built
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
