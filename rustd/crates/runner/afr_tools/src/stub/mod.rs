//! Stub handlers for the suites that prove routing and admission.
//!
//! A stub serves any published tool. A supervisor-side stub answers with its
//! name and never touches the executor; a sandbox-side one reads
//! [`STUB_PATH`] through it, so a suite can count that the call crossed.

use guffbolt::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};

use crate::catalog::Entry;
use crate::runtime::{CatalogTool, Runtime, ToolContext, ToolErrorCode};
use guffbolt::tools::{NoArguments, Schema};

/// The file a sandbox-side stub reads.
pub const STUB_PATH: &str = "stub.txt";

/// A handler that proves where it ran.
#[derive(Debug)]
pub struct Stub {
    entry: &'static Entry,
    schema: Schema,
}

impl Stub {
    /// A stub serving `entry`.
    #[must_use]
    pub fn new(entry: &'static Entry) -> Self {
        Self {
            entry,
            schema: Schema::of::<NoArguments>(entry.name()),
        }
    }

    /// The same, boxed for a catalog.
    #[must_use]
    pub fn boxed(entry: &'static Entry) -> Box<CatalogTool> {
        Box::new(Self::new(entry))
    }

    async fn run(&self, context: ToolContext<'_, '_>) -> ToolOutput {
        let name = self.entry.name();
        match (self.entry.runtime(), context.executor) {
            (Runtime::Sandbox, Some(executor)) => match executor.read_file(STUB_PATH, 1).await {
                Ok(_) => ToolOutput::succeeded(name),
                Err(failure) => {
                    ToolOutput::failed(ToolErrorCode::SandboxUnavailable, &failure.to_string())
                }
            },
            (Runtime::Sandbox, None) => ToolOutput::failed(ToolErrorCode::SandboxUnavailable, name),
            (Runtime::Supervisor | Runtime::Provider, _) => ToolOutput::succeeded(name),
        }
    }
}

impl Describe for Stub {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(self.entry.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Stub {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(self.run(cx))
    }
}

#[cfg(test)]
mod tests;
