//! How a child's end reaches its parent: an unknown id, input after the
//! end, a failure of the child's own, and an interrupt mid-call.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Error;
use afr_tools::Tool;
use afr_tools::catalog::{DELEGATE, HTTP_REQUEST, INTERRUPT_AGENT, SEND_INPUT, SPAWN, WAIT_AGENT};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    ACCEPTED, ANSWER, BRIEF_MS, CHILD_ID, DONE, INTERRUPTED, NEVER, OPENING, RUNNING, STALLS,
    STATUS, TASK, TIMEOUT_MS, last_result, offered as shared_names, parsed, requests_opening_with,
    tools as shared_tools,
};
use crate::fixture::{Canned, Script, Sent, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// An id no child of a run has.
const UNKNOWN: u64 = 9;

/// The six with the two, and a network tool that never answers.
fn tools() -> Vec<Box<dyn Tool>> {
    let mut tools = shared_tools();
    tools.push(Canned::boxed(&HTTP_REQUEST, ""));
    tools
}

fn offered() -> Vec<&'static str> {
    let mut names = shared_names();
    names.push(HTTP_REQUEST.name());
    names
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
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

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
        vec![call("p1", SPAWN.name(), json!({"task": TASK}))],
        vec![call("p2", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call(
            "p3",
            SEND_INPUT.name(),
            json!({CHILD_ID: 1, "message": "late"}),
        )],
        vec![say(DONE)],
    ])
    .with_child(TASK, [vec![say("x")]]);
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

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
        vec![call("p1", DELEGATE.name(), json!({"task": TASK}))],
        vec![say("the child failed, so I read the logs myself")],
    ])
    .with_failing_child(TASK, vec![say("partial ")], || Error::refused(503));
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

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

#[tokio::test(start_paused = true)]
async fn test_interrupting_a_child_mid_call_ends_its_call_once() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({"task": STALLS}))],
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
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

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
    let ended: Vec<_> = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(EVENT_CHILD_ENDED))
        .map(|event| event.field("status").map(str::to_owned))
        .collect();
    assert_eq!(ended, [Some("interrupted".to_owned())]);
}
