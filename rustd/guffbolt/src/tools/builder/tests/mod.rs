//! The builder: each tool built with its backend, offered as the startup
//! config names it, and refused when a named tool's backend was not given.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::fake::Fake;
use crate::tools::mcp::tool_name;
use crate::tools::outbound::Allowlist;
use crate::tools::{Backend, Context, ContextTool, Handler, Local, ToolsConfig, Typed, code};
use crate::{CallRef, Describe, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};

mod added;
mod backends;
mod offering;

/// Where the one call each case makes sits: the first of step 1.
fn first_call() -> CallRef {
    CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    }
}

/// A standalone run over `root`, open to any public host.
fn local_at(root: &std::path::Path) -> Local {
    Local::new("run-1", root, Allowlist::Any).expect("the directory exists")
}

/// The config naming `entries`.
fn config(entries: &[&str]) -> ToolsConfig {
    ToolsConfig::new(entries).expect("names and one-star patterns")
}

/// The names `tools` offers, in order.
fn names(tools: &Tools<ContextTool<'_>>) -> Vec<String> {
    tools.names().map(str::to_owned).collect()
}

/// A tool a server holds, standing in for one `Server::tools` offers: its
/// name is the server's, and it answers with that name.
#[derive(Debug)]
struct ServerTool {
    name: String,
    parameters: Value,
}

impl ServerTool {
    fn named(name: String) -> Self {
        Self {
            name,
            parameters: json!({"type": "object"}),
        }
    }
}

impl Describe for ServerTool {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: &self.name,
            description: "A tool the server holds.",
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Context<'c>> for ServerTool {
    fn call<'a>(&'a self, _arguments: &'a Value, _cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        Box::pin(std::future::ready(ToolOutput::succeeded(self.name.clone())))
    }
}
