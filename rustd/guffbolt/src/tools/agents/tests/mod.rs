//! Subagents end to end: the seven tools built through the builder, a root
//! that starts and steers children, and the harness driving each child's
//! turns over the embedder's conversation.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

mod fixture;

use serde_json::json;

use self::fixture::{Family, Play, Puppet, ROOT, Saving, agent, call, results, tools};
use crate::agents::{
    Agents, CLOSE_AGENT, Caps, Children, FOLLOWUP_TASK, Kind, LIST_AGENTS, SPAWN_AGENT, Status,
    TOOLS, WAIT_AGENT,
};
use crate::drive::{DriveState, drive};
use crate::extension::Plain;
use crate::step::Ending;
use crate::tool::Tools;
use crate::tools::{Backend, ContextTool, ToolsConfig};
use crate::transcript::{Message, Replay};

mod fork;
mod held;
mod resume;
mod steering;

/// The caps the cases run under, unless one says otherwise.
const CAPS: Caps = Caps {
    depth: 2,
    open: 2,
    started: 4,
};

fn user(text: &str) -> Message {
    Message::User(text.to_owned())
}

fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

/// The text of an answered ending.
fn answered(ending: &Ending<fixture::Exhausted>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

/// Drives the root of `family` from `state` over `opening`, its children
/// beside it, and ends the children once it ends; answers its ending and its
/// transcript.
async fn run_root(
    family: &Family<'_>,
    requests: crate::agents::Requests,
    opening: Vec<Message>,
    state: &mut DriveState,
) -> (Ending<fixture::Exhausted>, Vec<Message>) {
    let mut root = family.root(opening);
    let mut children = Children::new(requests);
    let ending = children
        .beside(family, drive(&mut root, &mut Plain, state))
        .await;
    let _quiet = children.end_all(&family.agents);
    (ending, root.transcript.messages().to_vec())
}
