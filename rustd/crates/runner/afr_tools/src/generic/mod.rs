//! `guffbolt`'s generic tools, as this runner hosts them: each call lent the
//! run's sandbox and egress guard through the harness's seams.
//!
//! ```text
//!  router ─► ToolContext { executor, run, call_ref } ─► Generic<T>::call
//!               executor ─► SandboxEnv ─┐
//!               run.egress ─► Egressed ─┴─► guffbolt::tools::Context ─► T::call
//! ```
//!
//! A tool built over a backend, such as the memory tools over
//! [`RunMemory`], took it when the run built it ([`crate::Backends`]) and
//! reads nothing from the context but where the call sits.
//!
//! A generic tool reaches the machine only through an `Env`, and here that
//! is only ever the run's sandbox: a runner host holds the provider's key,
//! so no tool built here runs a command or opens a file on it. A call the
//! router lends no executor has no environment at all, and a tool that needs
//! one refuses.

mod env;
mod memory;

use guffbolt::tools::{Context, ContextTool, Handler, Typed};
use guffbolt::{Describe, Tool, ToolFuture, ToolSpec};
use serde_json::Value;

use crate::egress::{Egressed, SharedTransport};
use crate::runtime::{CatalogTool, RunTool, ToolContext};

pub use self::env::SandboxEnv;
pub use self::memory::RunMemory;

/// A generic tool lent one run's state per call.
#[derive(Debug)]
pub(crate) struct Generic<T> {
    tool: T,
    /// Where its requests go once the run's guard admits them; `None` for
    /// a tool that sends none, whose every request is refused.
    transport: Option<SharedTransport>,
}

impl<H: Handler + 'static> Generic<Typed<H>> {
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

impl<'b> Generic<Box<ContextTool<'b>>> {
    /// `tool`, built for one run over the backends it calls, boxed for that
    /// run: it sends nothing through the context, so it gets no transport.
    #[must_use]
    pub(crate) fn built(tool: Box<ContextTool<'b>>) -> Box<RunTool<'b>> {
        Box::new(Self {
            tool,
            transport: None,
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
            // In the one repository the run holds; with several, the model
            // names one itself.
            let repository = match run.checkouts.as_slice() {
                [only] => Some(only.name),
                _none_or_several => None,
            };
            let context = Context {
                env: env.as_ref().map(|env| env as &dyn guffbolt::tools::Env),
                outbound: &outbound,
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
