//! `guffbolt`'s generic tools, as this runner hosts them: each built per run
//! over the run's sandbox, guard, memory and verbs, and lent the run's place
//! per call.
//!
//! ```text
//!  Backends ─► guffbolt's builder ─► each tool, holding its backend:
//!     executor source ─► LazySandboxEnv   run.egress ─► Egressed
//!  router ─► ToolContext { executor, run, call_ref } ─► Generic::call
//!               ─► guffbolt::tools::Context { call_ref, run_id } ─► the tool
//! ```
//!
//! A generic tool reaches the machine only through an `Env`, and here that
//! is only ever the run's sandbox: a runner host holds the provider's key,
//! so no tool built here runs a command or opens a file on it. A run with no
//! sandbox answers a sandbox call with why, in the environment's words.

mod env;
mod memory;

use guffbolt::tools::{Context, ContextTool};
use guffbolt::{Describe, Tool, ToolFuture, ToolSpec};
use serde_json::Value;

use crate::runtime::{RunTool, ToolContext};

pub use self::env::{LazySandboxEnv, SandboxEnv};
pub use self::memory::RunMemory;

/// A generic tool one run built over its backends, lent the run's place per
/// call.
#[derive(Debug)]
pub(crate) struct Generic<'b> {
    tool: Box<ContextTool<'b>>,
}

impl<'b> Generic<'b> {
    /// `tool`, built for one run over the backends it calls, boxed for that
    /// run.
    #[must_use]
    pub(crate) fn built(tool: Box<ContextTool<'b>>) -> Box<RunTool<'b>> {
        Box::new(Self { tool })
    }
}

impl Describe for Generic<'_> {
    fn spec(&self) -> ToolSpec<'_> {
        self.tool.spec()
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Generic<'_> {
    fn call<'a>(&'a self, arguments: &'a Value, cx: ToolContext<'call, 'run>) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        let context = Context {
            call_ref: cx.call_ref,
            run_id: cx.run.run_id,
            wake: None,
        };
        self.tool.call(arguments, context)
    }
}
