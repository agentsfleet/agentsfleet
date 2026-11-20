//! Subagents end to end: the seven tools built with `with_agents`, a root
//! that starts and steers children, and the harness driving each child's
//! turns over the embedder's conversation beside the root.

use guffbolt::agents::{
    Agents, CLOSE_AGENT, Caps, FOLLOWUP_TASK, INTERRUPT_AGENT, Kind, LIST_AGENTS, Listed,
    SEND_INPUT, SPAWN_AGENT, TOOLS, WAIT_AGENT,
};
use guffbolt::tools::{ContextTool, ToolsConfig};
use guffbolt::{DriveState, Plain, Tools};
use serde_json::json;

use crate::family::{Family, ROOT, agent, answered, said, user};
use crate::script::{Play, Script, call, results};

mod answers;
mod endings;
mod fork;
mod interrupts;
mod messages;
mod seats;

/// The caps the cases run under, unless one says otherwise.
const CAPS: Caps = Caps {
    depth: 2,
    open: 2,
    started: 4,
};

/// The seven subagent tools, and nothing else.
fn seven(agents: &Agents) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(TOOLS).expect("seven names");
    Tools::builder()
        .with_agents(agents)
        .build(&config)
        .expect("the run's children are given")
}
