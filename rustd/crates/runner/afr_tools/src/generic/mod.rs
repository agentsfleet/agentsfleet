//! `guffbolt`'s generic tools, as this runner hosts them: each call lent the
//! run's sandbox, egress guard and memory through the harness's seams.
//!
//! ```text
//!  router ─► ToolContext { executor, run, call_ref } ─► Generic<T>::call
//!               executor ─► SandboxEnv  ─┐
//!               run.egress ─► Egressed ├─► guffbolt::tools::Context ─► T::call
//!               run.memory ─► RunMemory   ─┘
//! ```
//!
//! A generic tool reaches the machine only through an `Env`, and here that
//! is only ever the run's sandbox: a runner host holds the provider's key,
//! so no tool built here runs a command or opens a file on it. A call the
//! router lends no executor has no environment at all, and a tool that needs
//! one refuses.

mod env;
mod memory;

use guffbolt::tools::{Context, Handler, Typed};
use guffbolt::{Describe, Tool, ToolFuture, ToolSpec};
use serde_json::Value;

use crate::egress::{Egressed, SharedTransport};
use crate::runtime::{CatalogTool, ToolContext};

pub use self::env::SandboxEnv;
use self::memory::RunMemory;

/// A generic tool lent one run's state per call.
#[derive(Debug)]
pub(crate) struct Generic<T> {
    tool: T,
    /// Where its requests go once the run's guard admits them; `None` for
    /// a tool that sends none, whose every request is refused.
    transport: Option<SharedTransport>,
}

impl<H: Handler> Generic<Typed<H>> {
    /// `handler`, which sends nothing, boxed for a catalog.
    #[must_use]
    pub(crate) fn boxed(handler: H) -> Box<CatalogTool> {
        Box::new(Self {
            tool: Typed::new(handler),
            transport: None,
        })
    }

    /// `handler`, its requests sent through `transport`, boxed for a catalog.
    #[must_use]
    pub(crate) fn sending(handler: H, transport: SharedTransport) -> Box<CatalogTool> {
        Box::new(Self {
            tool: Typed::new(handler),
            transport: Some(transport),
        })
    }
}

impl<T: Describe> Describe for Generic<T> {
    fn spec(&self) -> ToolSpec<'_> {
        self.tool.spec()
    }
}

impl<'call, 'run, T> Tool<ToolContext<'call, 'run>> for Generic<T>
where
    T: for<'c> Tool<Context<'c>>,
{
    fn call<'a>(&'a self, arguments: &'a Value, cx: ToolContext<'call, 'run>) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(async move {
            let run = cx.run;
            let env = cx.executor.map(SandboxEnv::new);
            let outbound = Egressed::new(self.transport.as_deref(), run);
            let memory = RunMemory::new(&run.memory);
            // In the one repository the run holds; with several, the model
            // names one itself.
            let repository = match run.checkouts.as_slice() {
                [only] => Some(only.name),
                _none_or_several => None,
            };
            let context = Context {
                env: env.as_ref().map(|env| env as &dyn guffbolt::tools::Env),
                outbound: &outbound,
                memory: &memory,
                sessions: &run.sessions,
                image_input: run.image_input,
                repository,
                call_ref: cx.call_ref,
                run_id: run.run_id,
                wake: None,
            };
            self.tool.call(arguments, context).await
        })
    }
}
