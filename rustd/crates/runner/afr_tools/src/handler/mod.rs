//! A tool whose arguments are a type.
//!
//! The model is told the JSON Schema of [`Handler::Arguments`] and a call's
//! arguments parse into it, so what the model is told and what the handler
//! reads cannot drift, and an argument the type does not name refuses the call
//! (`#[serde(deny_unknown_fields)]`), the way Codex's tools parse theirs
//! (`codex-rs/protocol/src/plan_tool.rs`). [`Typed`] is the one place a call
//! is parsed, for every handler.

use std::fmt;

use guffbolt::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::catalog::Entry;
use crate::runtime::{CatalogTool, ToolContext, parsed};
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
    pub(crate) fn boxed(handler: H) -> Box<CatalogTool> {
        Box::new(Self {
            handler,
            schema: Schema::of::<H::Arguments>(H::DESCRIPTION),
        })
    }
}

impl<H: Handler> Describe for Typed<H> {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(H::ENTRY.name())
    }
}

impl<'call, 'run, H: Handler> Tool<ToolContext<'call, 'run>> for Typed<H> {
    fn call<'a>(
        &'a self,
        arguments: &'a serde_json::Value,
        cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(async move {
            match parsed(arguments) {
                Ok(arguments) => self.handler.run(arguments, cx).await,
                Err(refused) => refused,
            }
        })
    }
}

#[cfg(test)]
mod tests;
