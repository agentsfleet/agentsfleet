//! What the nested suites share: the six with two tools that answer at
//! once, the names a lease here is offered, and how one loop's requests and
//! the run's frames and log are read back.

#![expect(
    clippy::unwrap_used,
    reason = "test fixture: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_wire::activity::ActivityFrame;
use afr_providers::Message;
use afr_tools::Tool;
use afr_tools::catalog::{MEMORY_RECALL, UPDATE_PLAN};
use afr_tools::nested::NESTED;

use crate::fixture::{Canned, Script, Sent};

/// What the fixture lease's event asks, which opens the root's conversation.
pub(super) const OPENING: &str = "triage the failed run";
pub(super) const READS: &str = "read a.md and b.md and summarise";
pub(super) const SUMMARY: &str = "summary";
pub(super) const TASK: &str = "check the logs";
pub(super) const ALSO: &str = "also c";
pub(super) const CHILD_DONE: &str = "child done";
pub(super) const STALLS: &str = "stall on the network";
/// What a root says once its children answered.
pub(super) const DONE: &str = "done";
/// What a root says when its round trip worked.
pub(super) const OK: &str = "ok";
/// What a loop would say, were it not ended first.
pub(super) const NEVER: &str = "never";
/// The argument naming a child.
pub(super) const CHILD_ID: &str = "child_id";
/// The argument bounding a wait.
pub(super) const TIMEOUT_MS: &str = "timeout_ms";
/// The argument naming a child's task.
pub(super) const TASK_KEY: &str = "task";
/// The fields the nested tools answer with.
pub(super) const STATUS: &str = "status";
pub(super) const ANSWER: &str = "answer";
pub(super) const ACCEPTED: &str = "accepted";
pub(super) const DEPTH: &str = "depth";
pub(super) const CALLS: &str = "calls";
/// The statuses a child reads back as.
pub(super) const RUNNING: &str = "running";
pub(super) const INTERRUPTED: &str = "interrupted";
/// A wait long enough for a child to reach its first call, in paused time.
pub(super) const BRIEF_MS: u64 = 1000;
/// How long a slow call stays open, in paused time.
pub(super) const SLOW_CALL: Duration = Duration::from_secs(10);

/// The six, with a plan tool and a recall tool that answer at once.
pub(super) fn tools() -> Vec<Box<dyn Tool>> {
    let mut tools = afr_tools::nested::tools();
    tools.push(Canned::boxed(&UPDATE_PLAN, "4"));
    tools.push(Canned::boxed(&MEMORY_RECALL, "recalled"));
    tools
}

/// Every name a lease here is offered: the six, and the two.
pub(super) fn offered() -> Vec<&'static str> {
    let mut names: Vec<&str> = NESTED.iter().map(|entry| entry.name()).collect();
    names.push(UPDATE_PLAN.name());
    names.push(MEMORY_RECALL.name());
    names
}

/// The requests whose conversation opened with `opening`: one loop's.
pub(super) fn requests_opening_with(script: &Script, opening: &str) -> Vec<Sent> {
    script
        .sent()
        .into_iter()
        .filter(|sent| {
            matches!(sent.messages.first(), Some(Message::User(text)) if text.starts_with(opening))
        })
        .collect()
}

/// Every tool result `sent` carries, in order.
pub(super) fn results(sent: &Sent) -> Vec<&str> {
    sent.messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.as_str()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect()
}

/// The call ids the start frames announced, in order.
pub(super) fn started_ids(frames: &[ActivityFrame<'_>]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallStarted(started) => {
                started.call_id.as_deref().map(str::to_owned)
            }
            _ => None,
        })
        .collect()
}

/// The events named `event`, in order.
pub(super) fn events(capture: &Capture, event: &str) -> Vec<CapturedEvent> {
    capture
        .events()
        .into_iter()
        .filter(|captured| captured.field("event") == Some(event))
        .collect()
}

/// The output of the last tool result `sent` carries: the call before it.
pub(super) fn last_result(sent: &Sent) -> &str {
    sent.messages
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.as_str()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .unwrap()
}

/// `text`, which a nested tool answered, as the JSON it is.
pub(super) fn parsed(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

/// The answer text the frames streamed, chunk by chunk.
pub(super) fn streamed(frames: &[ActivityFrame<'_>]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::FleetResponseChunk(chunk) => Some(chunk.text.to_string()),
            _ => None,
        })
        .collect()
}
