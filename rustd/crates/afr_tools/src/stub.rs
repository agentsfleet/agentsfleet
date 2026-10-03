//! Stub handlers for the suites that prove routing and admission.
//!
//! A stub serves any published tool. A supervisor-side stub answers with its
//! name and never touches the executor; a sandbox-side one reads
//! [`STUB_PATH`] through it, so a suite can count that the call crossed.

use crate::catalog::Entry;
use crate::runtime::{Runtime, Tool, ToolContext, ToolErrorCode, ToolOutput};
use crate::schema::Schema;

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
            schema: Schema {
                description: entry.name(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        }
    }

    /// The same, boxed for a catalog.
    #[must_use]
    pub fn boxed(entry: &'static Entry) -> Box<dyn Tool> {
        Box::new(Self::new(entry))
    }
}

#[async_trait::async_trait]
impl Tool for Stub {
    fn entry(&self) -> &'static Entry {
        self.entry
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(&self, _arguments: &serde_json::Value, context: ToolContext<'_>) -> ToolOutput {
        match (self.runtime(), context.executor) {
            (Runtime::Sandbox, Some(executor)) => match executor.read_file(STUB_PATH, 1).await {
                Ok(_) => ToolOutput::succeeded(self.name()),
                Err(failure) => {
                    ToolOutput::failed(ToolErrorCode::SandboxUnavailable, &failure.to_string())
                }
            },
            (Runtime::Sandbox, None) => {
                ToolOutput::failed(ToolErrorCode::SandboxUnavailable, self.name())
            }
            (Runtime::Supervisor | Runtime::Provider, _) => ToolOutput::succeeded(self.name()),
        }
    }
}
