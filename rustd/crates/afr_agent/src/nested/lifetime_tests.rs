//! No child outlives its parent: a child that answers takes every child it
//! started with it, and their slots with them.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::activity::ActivityFrame;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Chunk;
use afr_tools::catalog::{DELEGATE, SPAWN, WAIT_AGENT};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    BRIEF_MS, CHILD_DONE, CHILD_ID, DONE, FIRST_CALL, INTERRUPTED, NEVER, RUNNING, SECOND_CALL,
    STATUS, TASK_KEY, TIMEOUT_MS, events, last_result, parsed, requests_opening_with, results,
    root_requests, spawn_call, stall, stalling_offered, stalling_tools,
};
use super::registry::CHILDREN_RUNNING_MAX;
use crate::fixture::{Script, Sent, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// The root's delegated child, the grandchild it spawns, and the children
/// the root spawns after.
const PARENT: &str = "fan out";
const GRANDCHILD: &str = "stall below";
const LATER: &str = "stall later";
/// The ids the child and its grandchild take, and the first a later child
/// takes.
const PARENT_ID: &str = "1";
const GRANDCHILD_ID: u64 = 2;
const FIRST_LATER_ID: u64 = 3;
/// The call the grandchild leaves open: after the root's `delegate`, and
/// its parent's `spawn` and `wait_agent`.
const GRANDCHILD_CALL: &str = "4";

/// The root delegates to a child that spawns a grandchild, waits until the
/// grandchild is inside a call that never answers, and answers; the root
/// then peeks at the grandchild and spawns as many children as may run at
/// once.
fn fan_out() -> Script {
    let peek = json!({CHILD_ID: GRANDCHILD_ID, TIMEOUT_MS: BRIEF_MS});
    let later: Vec<Chunk> = (1..=CHILDREN_RUNNING_MAX)
        .map(|n| call(&spawn_call(n), SPAWN.name(), json!({TASK_KEY: LATER})))
        .collect();
    Script::new([
        vec![call(FIRST_CALL, DELEGATE.name(), json!({TASK_KEY: PARENT}))],
        vec![call(SECOND_CALL, WAIT_AGENT.name(), peek.clone())],
        later,
        vec![say(DONE)],
    ])
    .with_child(
        PARENT,
        [
            vec![call("c1", SPAWN.name(), json!({TASK_KEY: GRANDCHILD}))],
            vec![call("c2", WAIT_AGENT.name(), peek)],
            vec![say(CHILD_DONE)],
        ],
    )
    .with_child(GRANDCHILD, [stall(), vec![say(NEVER)]])
    .with_child(
        LATER,
        std::iter::repeat_with(stall).take(CHILDREN_RUNNING_MAX),
    )
}

/// What the root's later spawns answered, past the delegate's and the
/// peek's results.
fn spawned_later(root: &[Sent]) -> Vec<Value> {
    results(&root[3])[2..]
        .iter()
        .map(|text| parsed(text))
        .collect()
}

/// How the grandchild's open call ended, by the frames.
fn grandchild_call(frames: &[ActivityFrame<'_>]) -> Vec<(String, ToolCallStatus)> {
    completions(frames)
        .into_iter()
        .filter(|(id, _status)| id == GRANDCHILD_CALL)
        .collect()
}

/// How the child and its grandchild ended, by the log.
fn ended(capture: &Capture) -> Vec<(String, String)> {
    let grandchild = GRANDCHILD_ID.to_string();
    events(capture, EVENT_CHILD_ENDED)
        .iter()
        .filter_map(|event| {
            let id = event.field(CHILD_ID)?.to_owned();
            Some((id, event.field(STATUS)?.to_owned()))
        })
        .filter(|(id, _status)| id == PARENT_ID || *id == grandchild)
        .collect()
}

/// The child waits until its grandchild is inside a call that never
/// answers, then answers itself. The grandchild ends `interrupted` with it,
/// its open call ends once, and its slot is free: the root then starts as
/// many children as may run at once.
#[tokio::test(start_paused = true)]
async fn test_a_child_that_answers_interrupts_its_grandchild_and_frees_its_slot() {
    let capture = Capture::install();
    let script = fan_out();
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let parent = requests_opening_with(&script, PARENT);
    assert_eq!(
        parsed(last_result(&parent[2])),
        json!({STATUS: RUNNING}),
        "the grandchild was mid-call when its parent answered"
    );
    let root = root_requests(&script);
    assert_eq!(results(&root[1]), [CHILD_DONE]);
    assert_eq!(
        parsed(last_result(&root[2])),
        json!({STATUS: INTERRUPTED}),
        "ended with its parent, while the root still runs"
    );
    let admitted: Vec<Value> = (FIRST_LATER_ID..)
        .take(CHILDREN_RUNNING_MAX)
        .map(|id| json!({CHILD_ID: id}))
        .collect();
    assert_eq!(
        spawned_later(&root),
        admitted,
        "the grandchild's slot was freed"
    );
    assert_eq!(
        grandchild_call(&frames),
        [(GRANDCHILD_CALL.to_owned(), ToolCallStatus::Interrupted)]
    );
    assert_eq!(
        ended(&capture),
        [
            (PARENT_ID.to_owned(), DONE.to_owned()),
            (GRANDCHILD_ID.to_string(), INTERRUPTED.to_owned())
        ]
    );
}
