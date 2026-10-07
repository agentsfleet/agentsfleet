//! How a child's end reaches its parent: an unknown id, input after the
//! end, a failure of the child's own, and an interrupt mid-call.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::{Chunk, Error};
use afr_tools::catalog::{DELEGATE, HTTP_REQUEST, INTERRUPT_AGENT, SEND_INPUT, SPAWN, WAIT_AGENT};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    ACCEPTED, ANSWER, BRIEF_MS, CHILD_ID, DETAIL, DONE, FAILED, INTERRUPTED, NEVER, OPENING,
    RUNNING, STALLS, STATUS, TASK, TASK_KEY, TIMEOUT_MS, events, last_result, parsed,
    requests_opening_with, stalling_offered, stalling_tools,
};
use crate::fixture::{Script, Sent, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// An id no child of a run has.
const UNKNOWN: u64 = 9;
/// The status a failing child's provider answers with.
const UNAVAILABLE: u16 = 503;

/// The upstream fault a failing child's provider ends its turn on.
fn unavailable() -> Error {
    Error::refused(UNAVAILABLE)
}

/// The task of the `n`th stalling child.
fn stalling(n: u64) -> String {
    format!("{STALLS} {n}")
}

/// The root's requests, in order.
fn root_requests(script: &Script) -> Vec<Sent> {
    requests_opening_with(script, OPENING)
}

#[tokio::test]
async fn test_unknown_child_reads_not_found() {
    let script = Script::new([
        vec![call("p1", WAIT_AGENT.name(), json!({CHILD_ID: UNKNOWN}))],
        vec![call(
            "p2",
            SEND_INPUT.name(),
            json!({CHILD_ID: UNKNOWN, "message": "m"}),
        )],
        vec![call(
            "p3",
            INTERRUPT_AGENT.name(),
            json!({CHILD_ID: UNKNOWN}),
        )],
        vec![say(DONE)],
    ]);
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (_output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = root_requests(&script);
    for request in &root[1..=3] {
        let refused = last_result(request);
        assert!(refused.starts_with("[child_not_found]"), "{refused}");
        assert!(refused.contains("id 9"), "{refused}");
    }
    assert!(
        completions(&frames)
            .iter()
            .all(|(_id, status)| *status == ToolCallStatus::Failed)
    );
}

#[tokio::test]
async fn test_input_after_a_child_ended_is_not_accepted() {
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({TASK_KEY: TASK}))],
        vec![call("p2", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call(
            "p3",
            SEND_INPUT.name(),
            json!({CHILD_ID: 1, "message": "late"}),
        )],
        vec![say(DONE)],
    ])
    .with_child(TASK, [vec![say("x")]]);
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (_output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = root_requests(&script);
    assert_eq!(
        parsed(last_result(&root[2])),
        json!({STATUS: DONE, ANSWER: "x"})
    );
    assert_eq!(parsed(last_result(&root[3])), json!({ACCEPTED: false}));
}

#[tokio::test]
async fn test_a_delegated_childs_failure_is_the_calls_failure() {
    let script = Script::new([
        vec![call("p1", DELEGATE.name(), json!({TASK_KEY: TASK}))],
        vec![say("the child failed, so I read the logs myself")],
    ])
    .with_failing_child(TASK, vec![say("partial ")], unavailable);
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = root_requests(&script);
    let failed = last_result(&root[1]);
    assert!(failed.starts_with("[child_failed]"), "{failed}");
    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Failed)]
    );
    assert_eq!(
        output.result.content, "the child failed, so I read the logs myself",
        "the parent decides"
    );
}

/// A spawned child that fails leaves its failure for `wait_agent`, which
/// answers the child's status and its detail; the parent decides.
#[tokio::test]
async fn test_wait_agent_returns_a_spawned_childs_failure() {
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({TASK_KEY: TASK}))],
        vec![call("p2", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![say(DONE)],
    ])
    .with_failing_child(TASK, vec![say("partial ")], unavailable);
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = root_requests(&script);
    assert_eq!(
        parsed(last_result(&root[2])),
        json!({STATUS: FAILED, DETAIL: unavailable().detail()})
    );
    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), ToolCallStatus::Succeeded),
            ("2".to_owned(), ToolCallStatus::Succeeded)
        ],
        "the wait read the failure; it did not fail itself"
    );
    assert_eq!(output.result.content, DONE, "the parent decides");
}

#[tokio::test(start_paused = true)]
async fn test_interrupting_a_child_mid_call_ends_its_call_once() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({TASK_KEY: STALLS}))],
        vec![call(
            "p2",
            WAIT_AGENT.name(),
            json!({CHILD_ID: 1, TIMEOUT_MS: BRIEF_MS}),
        )],
        vec![call("p3", INTERRUPT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![say(DONE)],
    ])
    .with_child(
        STALLS,
        [
            vec![call("c1", HTTP_REQUEST.name(), json!({}))],
            vec![say(NEVER)],
        ],
    );
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
    let ended: Vec<_> = events(&capture, EVENT_CHILD_ENDED)
        .iter()
        .map(|event| event.field(STATUS).map(str::to_owned))
        .collect();
    assert_eq!(ended, [Some("interrupted".to_owned())]);
}

/// A child's slot is freed once: by `interrupt_agent`, and not again when
/// the interrupted child's loop is polled to its end. Four stalling
/// children fill the cap; interrupting one admits a fifth; once the
/// interrupted one has ended, a sixth is still refused.
#[tokio::test(start_paused = true)]
async fn test_an_interrupted_childs_slot_is_freed_once() {
    let capture = Capture::install();
    let spawns: Vec<Chunk> = (1..=4)
        .map(|n| {
            call(
                &format!("s{n}"),
                SPAWN.name(),
                json!({TASK_KEY: stalling(n)}),
            )
        })
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
        script = script.with_child(
            &stalling(n),
            [
                vec![call("c", HTTP_REQUEST.name(), json!({}))],
                vec![say(NEVER)],
            ],
        );
    }
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
    let ended: Vec<_> = events(&capture, EVENT_CHILD_ENDED)
        .iter()
        .map(|event| {
            (
                event.field(CHILD_ID).map(str::to_owned),
                event.field(STATUS).map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        ended
            .iter()
            .filter(|(id, _status)| id.as_deref() == Some("1"))
            .count(),
        1,
        "the interrupted child ends once in the log: {ended:?}"
    );
}
