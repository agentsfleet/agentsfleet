//! The built-in tools that need little or no backend: asking the person, the
//! plan, the clock, memory, a line to the conversation, a proposal and a
//! one-off schedule, each through `Tools::builder()` and `dispatch`, and a
//! handler of the embedder's own made a tool with `Typed`.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use guffbolt::agents::{Agents, Caps, SPAWN_AGENT, TOOLS, WAIT_AGENT};
use guffbolt::tools::memory::{
    Forgot, Memory, MemoryFuture, MemoryRefused, MemoryStore, Reach, Remembered,
};
use guffbolt::tools::propose::{Document, Kept, Propose, Refusal};
use guffbolt::tools::{
    Backend, Context, ContextTool, Handler, LocalChannel, LocalMemory, LocalScheduler, Schema,
    ToolsConfig, Typed, Wake, code,
};
use guffbolt::{
    CallRef, Describe, DriveState, Ending, ErrorCode, Plain, ToolOutput, ToolSpec, Tools,
};
use serde_json::{Value, json};

use crate::family::{Family, ROOT, agent, at, user};
use crate::script::{Play, Script, call, lock, results};

mod ask_plan_clock;
mod cron;
mod handlers;
mod memory;

const AT: CallRef = CallRef {
    step: 1,
    index: 0,
    call_id: String::new(),
};

/// Runs `name` once over `tools`, lent `wake`.
async fn run(
    tools: &Tools<ContextTool<'_>>,
    name: &str,
    arguments: Value,
    wake: Option<&Wake>,
) -> ToolOutput {
    let context = Context {
        call_ref: &AT,
        run_id: "run-1",
        wake,
    };
    tools.dispatch(name, &arguments, context).await
}

fn built<'r>(builder: guffbolt::tools::ToolsBuilder<'r>, names: &[&str]) -> Tools<ContextTool<'r>> {
    builder
        .build(&ToolsConfig::new(names).expect("names"))
        .expect("built")
}
