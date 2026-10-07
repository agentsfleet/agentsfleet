//! Children are visible as the run's own calls, and every child ends with
//! its parent.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_wire::report::ResultOutcome;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Chunk;
use afr_tools::catalog::{
    DELEGATE, HTTP_REQUEST, INTERRUPT_AGENT, LIST_AGENTS, MEMORY_RECALL, SPAWN, UPDATE_PLAN,
    WAIT_AGENT,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    CALLS, CHILD_ID, DEPTH, DONE, INTERRUPTED, NEVER, OPENING, READS, RUNNING, STALLS, STATUS,
    SUMMARY, events, offered, parsed, requests_opening_with, results, started_ids, tools,
};
use crate::fixture::{Canned, Script, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// Calls the parent makes beside one `delegate`, so the parent makes 150.
const PARENT_CALLS: usize = 149;
/// Calls the delegated child makes.
const CHILD_CALLS: usize = 60;

#[tokio::test]
async fn test_child_calls_share_the_run_trace() {
    let script = Script::new([
        vec![call("p1", DELEGATE.name(), json!({"task": READS}))],
        vec![say(DONE)],
    ])
    .with_child(
        READS,
        [
            vec![
                call("c1", UPDATE_PLAN.name(), json!({})),
                call("c2", MEMORY_RECALL.name(), json!({})),
            ],
            vec![say(SUMMARY)],
        ],
    );
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(started_ids(&frames), ["1", "2", "3"], "one counter");
    let succeeded = ToolCallStatus::Succeeded;
    assert_eq!(
        completions(&frames),
        [
            ("2".to_owned(), succeeded),
            ("3".to_owned(), succeeded),
            ("1".to_owned(), succeeded)
        ],
        "the child's calls end inside the parent's"
    );
    let trace = output.trace.unwrap();
    let rows: Vec<(&str, &str)> = trace
        .calls
        .iter()
        .map(|row| (row.call_id.as_ref(), row.name.as_ref()))
        .collect();
    assert_eq!(
        rows,
        [
            ("2", UPDATE_PLAN.name()),
            ("3", MEMORY_RECALL.name()),
            ("1", DELEGATE.name())
        ]
    );
    assert_eq!(output.records.len(), 3);
}

#[tokio::test]
async fn test_trace_cap_counts_child_calls() {
    let mut parent: Vec<Chunk> = (1..=PARENT_CALLS)
        .map(|n| call(&format!("p{n}"), UPDATE_PLAN.name(), json!({})))
        .collect();
    parent.push(call("pd", DELEGATE.name(), json!({"task": READS})));
    let child: Vec<Chunk> = (1..=CHILD_CALLS)
        .map(|n| call(&format!("c{n}"), UPDATE_PLAN.name(), json!({})))
        .collect();
    let script =
        Script::new([parent, vec![say(DONE)]]).with_child(READS, [child, vec![say(SUMMARY)]]);
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let trace = output.trace.unwrap();
    assert_eq!(trace.omitted_call_count, 10);
    assert_eq!(trace.calls.len(), 200);
}

#[tokio::test(start_paused = true)]
async fn test_parent_end_interrupts_children() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({"task": STALLS}))],
        vec![call("p2", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![say(NEVER)],
    ])
    .with_child(
        STALLS,
        [
            vec![call("c1", HTTP_REQUEST.name(), json!({}))],
            vec![say(NEVER)],
        ],
    );
    let mut tools = tools();
    tools.push(Canned::boxed(&HTTP_REQUEST, ""));
    let engine = engine(tools, &script);
    let mut names = offered();
    names.push(HTTP_REQUEST.name());
    let lease = lease(&names, unbounded());
    let stop = CancellationToken::new();
    let stopper = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        stop.cancel();
    };

    let ((output, frames), ()) = tokio::join!(drive(&engine, &lease, &stop), stopper);

    let interrupted = ToolCallStatus::Interrupted;
    assert_eq!(
        completions(&frames),
        [
            ("1".to_owned(), ToolCallStatus::Succeeded),
            ("2".to_owned(), interrupted),
            ("3".to_owned(), interrupted)
        ],
        "the parent's wait and the child's open call, once each"
    );
    let trace = output.trace.unwrap();
    assert_eq!(trace.calls.len(), 3);
    let ResultOutcome::Failed(_stopped) = output.result.outcome else {
        panic!("a stopped run is not a completed one");
    };
    let ended = events(&capture, EVENT_CHILD_ENDED);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].field("status"), Some("interrupted"));
}

#[tokio::test]
async fn test_interrupt_and_list_agents() {
    let script = Script::new([
        vec![
            call("p1", SPAWN.name(), json!({"task": "a"})),
            call("p2", SPAWN.name(), json!({"task": "b"})),
        ],
        vec![call("p3", INTERRUPT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call("p4", LIST_AGENTS.name(), json!({}))],
        vec![say(DONE)],
    ])
    .with_child("a", [vec![say("x")]])
    .with_child("b", [vec![say("x")]]);
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let root = requests_opening_with(&script, OPENING);
    let spawned: Vec<serde_json::Value> = results(&root[1]).into_iter().map(parsed).collect();
    assert_eq!(spawned, [json!({CHILD_ID: 1}), json!({CHILD_ID: 2})]);
    assert_eq!(parsed(results(&root[2])[2]), json!({STATUS: INTERRUPTED}));
    assert_eq!(
        parsed(results(&root[3])[3]),
        json!([
            {CHILD_ID: 1, STATUS: INTERRUPTED, DEPTH: 1, CALLS: 0},
            {CHILD_ID: 2, STATUS: RUNNING, DEPTH: 1, CALLS: 0}
        ])
    );
}
