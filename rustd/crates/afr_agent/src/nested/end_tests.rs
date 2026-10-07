//! How a child's end reaches its parent: an unknown id, input after the
//! end, and a failure of the child's own.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Error;
use afr_tools::catalog::{DELEGATE, INTERRUPT_AGENT, SEND_INPUT, SPAWN, WAIT_AGENT};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::fixture::{
    ACCEPTED, ANSWER, CHILD_ID, DETAIL, DONE, FAILED, FIRST_CALL, SECOND_CALL, STATUS, TASK,
    TASK_KEY, THIRD_CALL, last_result, parsed, root_requests, stalling_offered, stalling_tools,
};
use crate::fixture::{Script, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// An id no child of a run has.
const UNKNOWN: u64 = 9;
/// The status a failing child's provider answers with.
const UNAVAILABLE: u16 = 503;

/// The upstream fault a failing child's provider ends its turn on.
fn unavailable() -> Error {
    Error::refused(UNAVAILABLE)
}

#[tokio::test]
async fn test_unknown_child_reads_not_found() {
    let script = Script::new([
        vec![call(
            FIRST_CALL,
            WAIT_AGENT.name(),
            json!({CHILD_ID: UNKNOWN}),
        )],
        vec![call(
            SECOND_CALL,
            SEND_INPUT.name(),
            json!({CHILD_ID: UNKNOWN, "message": "m"}),
        )],
        vec![call(
            THIRD_CALL,
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
        vec![call(FIRST_CALL, SPAWN.name(), json!({TASK_KEY: TASK}))],
        vec![call(SECOND_CALL, WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call(
            THIRD_CALL,
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
        vec![call(FIRST_CALL, DELEGATE.name(), json!({TASK_KEY: TASK}))],
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
        vec![call(FIRST_CALL, SPAWN.name(), json!({TASK_KEY: TASK}))],
        vec![call(SECOND_CALL, WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
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
