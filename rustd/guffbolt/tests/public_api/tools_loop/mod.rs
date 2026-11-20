//! A loop over tools `Tools::builder()` built: backends given with the
//! `with_*` setters, a tool of the embedder's own beside them, and every way
//! the builder or a call refuses.

use guffbolt::agents::{Agents, Caps};
use guffbolt::tools::memory::Memory;
use guffbolt::tools::{
    Backend, Context, ContextTool, LocalChannel, LocalEnv, LocalMemory, LocalScheduler, Sessions,
    ToolsConfig, built_in,
};
use guffbolt::{Describe, DriveState, Plain, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};
use serde_json::{Value, json};

use crate::family::{Family, ROOT, answered, user};
use crate::script::{Play, Script, call, results};

mod building;
mod runs;

const CAPS: Caps = Caps {
    depth: 1,
    open: 1,
    started: 1,
};

/// A tool of the embedder's own, answering with the call's place.
#[derive(Debug)]
struct Echo {
    name: &'static str,
    parameters: Value,
}

impl Describe for Echo {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: self.name,
            description: "Echoes its call.",
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Context<'c>> for Echo {
    fn call<'a>(&'a self, arguments: &'a Value, cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        let said = format!(
            "{} {} at step {} call {}",
            self.name, arguments, cx.call_ref.step, cx.call_ref.index
        );
        Box::pin(async move { ToolOutput::succeeded(said) })
    }
}

fn echo(name: &'static str) -> Echo {
    Echo {
        name,
        parameters: json!({"type": "object"}),
    }
}
