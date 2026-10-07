//! Interrupting a child: mid-call, its slot freed once, and after it had
//! already ended.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Chunk;
use afr_tools::ToolErrorCode;
use afr_tools::catalog::{DELEGATE, HTTP_REQUEST, INTERRUPT_AGENT, SPAWN, WAIT_AGENT};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    BRIEF_MS, CHILD_DONE, CHILD_ID, DONE, FIRST_CALL, INTERRUPTED, NEVER, RUNNING, SECOND_CALL,
    STALLS, STATUS, TASK, TASK_KEY, THIRD_CALL, TIMEOUT_MS, events, last_result, parsed,
    root_requests, spawn_call, stall, stalling_offered, stalling_tools,
};
use super::registry::CHILDREN_RUNNING_MAX;
use crate::fixture::{Script, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// The first child's id, as the log spells it.
const FIRST_CHILD: &str = "1";

/// The task of the `n`th stalling child.
fn stalling(n: u64) -> String {
    format!("{STALLS} {n}")
}

/// A turn opening a network call that never answers, then what the child
/// would say were it not ended first.
fn stalls_then_never(call_id: &str) -> [Vec<Chunk>; 2] {
    [
        vec![call(call_id, HTTP_REQUEST.name(), json!({}))],
        vec![say(NEVER)],
    ]
}

/// Each child's end in the log: its id and status.
fn ends(capture: &Capture) -> Vec<(Option<String>, Option<String>)> {
    events(capture, EVENT_CHILD_ENDED)
        .iter()
        .map(|event| {
            (
                event.field(CHILD_ID).map(str::to_owned),
                event.field(STATUS).map(str::to_owned),
            )
        })
        .collect()
}

/// The statuses child `id` ended with, by the log.
fn ends_of(capture: &Capture, id: &str) -> Vec<Option<String>> {
    ends(capture)
        .into_iter()
        .filter(|(child, _status)| child.as_deref() == Some(id))
        .map(|(_child, status)| status)
        .collect()
}

/// The root spawns a child that opens a call that never answers, waits on it
/// briefly, then interrupts it.
fn interrupted_mid_call() -> Script {
    Script::new([
        vec![call(FIRST_CALL, SPAWN.name(), json!({TASK_KEY: STALLS}))],
        vec![call(
            SECOND_CALL,
            WAIT_AGENT.name(),
            json!({CHILD_ID: 1, TIMEOUT_MS: BRIEF_MS}),
        )],
        vec![call(
            THIRD_CALL,
            INTERRUPT_AGENT.name(),
            json!({CHILD_ID: 1}),
        )],
        vec![say(DONE)],
    ])
    .with_child(STALLS, stalls_then_never("c1"))
}

#[tokio::test(start_paused = true)]
async fn test_interrupting_a_child_mid_call_ends_its_call_once() {
    let capture = Capture::install();
    let script = interrupted_mid_call();
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = root_requests(&script);
    assert_eq!(
        parsed(last_result(&root[2])),
        json!({STATUS: RUNNING}),
        "timed out"
    );
    assert_eq!(parsed(last_result(&root[3])), json!({STATUS: INTERRUPTED}));
    assert_eq!(output.result.content, DONE);
    let succeeded = ToolCallStatus::Succeeded;
    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), succeeded),
            ("2".to_owned(), succeeded),
            ("4".to_owned(), succeeded),
            ("3".to_owned(), ToolCallStatus::Interrupted)
        ],
        "the child's open call ends once, when the run drops it"
    );
    assert_eq!(
        ends(&capture),
        [(Some(FIRST_CHILD.to_owned()), Some(INTERRUPTED.to_owned()))]
    );
}

/// Four stalling children fill the cap; the root interrupts the first,
/// spawns a fifth, waits on the third while the interrupted one is polled to
/// its end, then tries a sixth.
fn fill_then_interrupt() -> Script {
    let spawns: Vec<Chunk> = (1..=4)
        .map(|n| call(&spawn_call(n), SPAWN.name(), json!({TASK_KEY: stalling(n)})))
        .collect();
    let mut script = Script::new([
        spawns,
        vec![call("i1", INTERRUPT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call("s5", SPAWN.name(), json!({TASK_KEY: stalling(5)}))],
        vec![call(
            "w3",
            WAIT_AGENT.name(),
            json!({CHILD_ID: 3, TIMEOUT_MS: BRIEF_MS}),
        )],
        vec![call("s6", SPAWN.name(), json!({TASK_KEY: stalling(6)}))],
        vec![say(DONE)],
    ]);
    for n in 1..=6 {
        script = script.with_child(&stalling(n), stalls_then_never("c"));
    }
    script
}

/// A child's slot is freed once: by `interrupt_agent`, and not again when
/// the interrupted child's loop is polled to its end. Four stalling
/// children fill the cap; interrupting one admits a fifth; once the
/// interrupted one has ended, a sixth is still refused.
#[tokio::test(start_paused = true)]
async fn test_an_interrupted_childs_slot_is_freed_once() {
    let capture = Capture::install();
    let script = fill_then_interrupt();
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let root = root_requests(&script);
    assert_eq!(parsed(last_result(&root[2])), json!({STATUS: INTERRUPTED}));
    assert_eq!(
        parsed(last_result(&root[3])),
        json!({CHILD_ID: 5}),
        "a slot freed"
    );
    assert_eq!(
        parsed(last_result(&root[4])),
        json!({STATUS: RUNNING}),
        "the interrupted child was polled to its end meanwhile"
    );
    let sixth = last_result(&root[5]);
    assert!(
        sixth.starts_with("[child_cap_reached]"),
        "freed twice: {sixth}"
    );
    assert_eq!(
        ends_of(&capture, FIRST_CHILD).len(),
        1,
        "the interrupted child ends once in the log: {:?}",
        ends(&capture)
    );
}

/// Interrupting a child that already answered keeps the end it had: the
/// interrupt reads `done`, the child ends once in the log, and its slot,
/// freed when it answered, is not freed again. With the cap filled after it,
/// the interrupt admits no further child.
#[tokio::test(start_paused = true)]
async fn test_interrupting_a_finished_child_keeps_its_end() {
    let capture = Capture::install();
    let fill: Vec<Chunk> = (1..=CHILDREN_RUNNING_MAX)
        .map(|n| call(&spawn_call(n), SPAWN.name(), json!({TASK_KEY: STALLS})))
        .collect();
    let script = Script::new([
        vec![call(FIRST_CALL, DELEGATE.name(), json!({TASK_KEY: TASK}))],
        fill,
        vec![call(
            SECOND_CALL,
            INTERRUPT_AGENT.name(),
            json!({CHILD_ID: 1}),
        )],
        vec![call(THIRD_CALL, SPAWN.name(), json!({TASK_KEY: STALLS}))],
        vec![say(DONE)],
    ])
    .with_child(TASK, [vec![say(CHILD_DONE)]])
    .with_child(
        STALLS,
        std::iter::repeat_with(stall).take(CHILDREN_RUNNING_MAX + 1),
    );
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let root = root_requests(&script);
    assert_eq!(
        parsed(last_result(&root[3])),
        json!({STATUS: DONE}),
        "the end it had"
    );
    let refused = last_result(&root[4]);
    let capped = format!("[{}]", ToolErrorCode::ChildCapReached.as_str());
    assert!(refused.starts_with(&capped), "freed twice: {refused}");
    assert_eq!(
        ends_of(&capture, FIRST_CHILD),
        [Some(DONE.to_owned())],
        "it ends once in the log"
    );
}
