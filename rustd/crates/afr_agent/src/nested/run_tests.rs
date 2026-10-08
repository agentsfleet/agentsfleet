//! Children are visible as the run's own calls, and every child ends with
//! its parent.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use std::sync::Arc;

use crate::ResultOutcome;
use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_egress::testing::{CountingMint, RecordingTransport};
use afr_providers::Chunk;
use afr_tools::Catalog;
use afr_tools::catalog::{
    DELEGATE, HTTP_REQUEST, INTERRUPT_AGENT, LIST_AGENTS, MEMORY_RECALL, MEMORY_STORE, SPAWN,
    UPDATE_PLAN, WAIT_AGENT,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_ENDED;
use super::fixture::{
    ANSWER, CALLS, CHILD_DONE, CHILD_ID, DEPTH, DONE, INTERRUPTED, NEVER, OPENING, READS, RUNNING,
    STALLS, STATUS, SUMMARY, events, offered, parsed, requests_opening_with, results, stall,
    stalling_offered, stalling_tools, started_ids, streamed, tools,
};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::fixture::{Canned, Frames, Script, call, lease, say, unbounded};
use crate::harness::Loop;
use crate::harness::tests::{completions, drive, engine};
use crate::testing::Recording;

/// Calls the parent makes beside one `delegate`, so the parent makes 150.
const PARENT_CALLS: usize = 149;
/// Calls the delegated child makes.
const CHILD_CALLS: usize = 60;
/// Two children that stall in a call, and one that answers at once.
const FIRST: &str = "stall first";
const SECOND: &str = "answer at once";
const THIRD: &str = "stall third";
/// The id the third child takes.
const THIRD_ID: &str = "3";

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
    assert_eq!(
        streamed(&frames),
        [DONE],
        "a child's answer is its parent's to read, never the thread's"
    );
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

/// Three children: the first and the third stall in a call that never
/// answers, the second answers at once. Waiting on the second leaves it
/// `done` and interrupting the first leaves it `interrupted` before the list
/// is read, in whatever order the children were polled; the third runs until
/// the root answers, and then ends `interrupted` with it.
#[tokio::test]
async fn test_interrupt_and_list_agents() {
    let capture = Capture::install();
    let script = Script::new([
        vec![
            call("p1", SPAWN.name(), json!({"task": FIRST})),
            call("p2", SPAWN.name(), json!({"task": SECOND})),
            call("p3", SPAWN.name(), json!({"task": THIRD})),
        ],
        vec![call("p4", WAIT_AGENT.name(), json!({CHILD_ID: 2}))],
        vec![call("p5", INTERRUPT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![call("p6", LIST_AGENTS.name(), json!({}))],
        vec![say(DONE)],
    ])
    .with_child(FIRST, [stall(), vec![say(NEVER)]])
    .with_child(SECOND, [vec![say(CHILD_DONE)]])
    .with_child(THIRD, [stall(), vec![say(NEVER)]]);
    let engine = engine(stalling_tools(), &script);
    let lease = lease(&stalling_offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let root = requests_opening_with(&script, OPENING);
    let spawned: Vec<serde_json::Value> = results(&root[1]).into_iter().map(parsed).collect();
    assert_eq!(
        spawned,
        [
            json!({CHILD_ID: 1}),
            json!({CHILD_ID: 2}),
            json!({CHILD_ID: 3})
        ]
    );
    assert_eq!(
        parsed(results(&root[2])[3]),
        json!({STATUS: DONE, ANSWER: CHILD_DONE})
    );
    assert_eq!(parsed(results(&root[3])[4]), json!({STATUS: INTERRUPTED}));
    assert_eq!(
        parsed(results(&root[4])[5]),
        json!([
            {CHILD_ID: 1, STATUS: INTERRUPTED, DEPTH: 1, CALLS: 0},
            {CHILD_ID: 2, STATUS: DONE, DEPTH: 1, CALLS: 0},
            {CHILD_ID: 3, STATUS: RUNNING, DEPTH: 1, CALLS: 0}
        ])
    );
    let third: Vec<Option<String>> = events(&capture, EVENT_CHILD_ENDED)
        .iter()
        .filter(|event| event.field(CHILD_ID) == Some(THIRD_ID))
        .map(|event| event.field(STATUS).map(str::to_owned))
        .collect();
    assert_eq!(
        third,
        [Some(INTERRUPTED.to_owned())],
        "the child still running ends with the root's answer"
    );
}

/// A child writes no checkpoint of its own: what it stores rides the
/// parent's next checkpoint, so a cadence of one call pushes once for the
/// `delegate` call and never a second time for the child's store.
#[tokio::test]
async fn test_a_child_writes_no_checkpoint_of_its_own() {
    let script = Script::new([
        vec![call("p1", DELEGATE.name(), json!({"task": READS}))],
        vec![say(DONE)],
    ])
    .with_child(
        READS,
        [
            vec![call(
                "c1",
                MEMORY_STORE.name(),
                json!({"key": "found", "content": "x"}),
            )],
            vec![say(SUMMARY)],
        ],
    );
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let every_call = json!({"tool_window": 0, "memory_checkpoint_every": 1,
        "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0});
    let lease = lease(&[MEMORY_STORE.name(), DELEGATE.name()], every_call);
    let (checkpoint, pushed) = Recording::new();
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &checkpoint,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();

    frames.taken();
    let written: Vec<Vec<String>> = pushed
        .try_iter()
        .map(|push| push.iter().map(|delta| delta.key.to_string()).collect())
        .collect();
    assert_eq!(
        written,
        [vec!["found".to_owned()]],
        "one push, after the delegate call"
    );
    assert_eq!(output.memory.len(), 1, "the final push carries it again");
}

/// A nested child run opens with its task alone: the parent's earlier turns
/// are the parent's conversation, never the child's.
#[tokio::test]
async fn test_child_run_carries_no_history() {
    let script = Script::new([
        vec![call("p1", DELEGATE.name(), json!({"task": READS}))],
        vec![say(DONE)],
    ])
    .with_child(READS, [vec![say(SUMMARY)]]);
    let engine = engine(tools(), &script);
    let mut lease = lease(&offered(), unbounded());
    lease.history = vec![afd_wire::lease::Turn {
        message: "which tests failed?".into(),
        answer: "two".into(),
    }];

    drive(&engine, &lease, &CancellationToken::new()).await;

    let child_first = script
        .sent()
        .into_iter()
        .find(|sent| sent.messages.first() == Some(&afr_providers::Message::User(READS.to_owned())))
        .unwrap();
    assert_eq!(
        child_first.messages,
        [afr_providers::Message::User(READS.to_owned())],
        "the child's first request holds its task alone"
    );
}
