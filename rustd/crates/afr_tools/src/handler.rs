//! A tool whose arguments are a type.
//!
//! The model is told the JSON Schema of [`Handler::Arguments`] and a call's
//! arguments parse into it, so what the model is told and what the handler
//! reads cannot drift, and an argument the type does not name refuses the call
//! (`#[serde(deny_unknown_fields)]`), the way Codex's tools parse theirs
//! (`codex-rs/protocol/src/plan_tool.rs`). [`Typed`] is the one place a call
//! is parsed, for every handler.

use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize as _;
use serde::de::DeserializeOwned;

use crate::catalog::Entry;
use crate::runtime::{Tool, ToolContext, ToolErrorCode, ToolOutput};
use crate::schema::Schema;

/// One tool's behaviour, given arguments that parsed.
#[async_trait::async_trait]
pub(crate) trait Handler: Send + Sync + fmt::Debug + 'static {
    /// The published tool it serves.
    const ENTRY: &'static Entry;
    /// What the tool does, as the model reads it.
    const DESCRIPTION: &'static str;
    /// What one call carries.
    type Arguments: DeserializeOwned + JsonSchema + Send;

    /// Runs one call.
    async fn run(&self, arguments: Self::Arguments, context: ToolContext<'_, '_>) -> ToolOutput;
}

/// A [`Handler`] as the catalog holds it: its schema, built once.
#[derive(Debug)]
pub(crate) struct Typed<H> {
    handler: H,
    schema: Schema,
}

impl<H: Handler> Typed<H> {
    /// `handler`, boxed for a catalog.
    #[must_use]
    pub(crate) fn boxed(handler: H) -> Box<dyn Tool> {
        Box::new(Self {
            handler,
            schema: Schema::of::<H::Arguments>(H::DESCRIPTION),
        })
    }
}

#[async_trait::async_trait]
impl<H: Handler> Tool for Typed<H> {
    fn entry(&self) -> &'static Entry {
        H::ENTRY
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(&self, arguments: &serde_json::Value, context: ToolContext<'_, '_>) -> ToolOutput {
        match H::Arguments::deserialize(arguments) {
            Ok(parsed) => self.handler.run(parsed, context).await,
            Err(refused) => ToolOutput::failed(ToolErrorCode::InvalidArguments, &refused.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "handler/tests.rs"]
mod tests;
