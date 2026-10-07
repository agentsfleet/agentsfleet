//! Children share the lease and can only have less: a delegated answer, a
//! spawned round trip, the depth cap, the run's caps, the tool subset, and
//! usage summed into the one report.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::{Chunk, Message};
use afr_tools::catalog::{
    DELEGATE, HTTP_REQUEST, MEMORY_RECALL, SEND_INPUT, SPAWN, UPDATE_PLAN, WAIT_AGENT,
};
use afr_tools::nested::NESTED;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::{EVENT_CHILD_ENDED, EVENT_CHILD_STARTED};
use super::fixture::{
    ACCEPTED, ALSO, ANSWER, BRIEF_MS, CHILD_DONE, CHILD_ID, DONE, OK, OPENING, READS, RUNNING,
    SLOW_CALL, STATUS, SUMMARY, TASK, TIMEOUT_MS, events, offered, parsed, requests_opening_with,
    results, tools,
};
use super::start::EVENT_CHILD_REFUSED;
use crate::fixture::{Script, Slow, call, lease, say, spent, unbounded};
use crate::harness::tests::{completions, drive, engine};

#[tokio::test]
async fn test_delegate_returns_child_answer() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call(
            "p1",
            DELEGATE.name(),
            json!({"task": READS, "tools": [UPDATE_PLAN.name(), MEMORY_RECALL.name()]}),
        )],
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

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = requests_opening_with(&script, OPENING);
    assert_eq!(
        results(&root[1]),
        [SUMMARY],
        "the child's answer is the call's output"
    );
    assert_eq!(output.result.content, DONE);
    let child = requests_opening_with(&script, READS);
    assert_eq!(child.len(), 2, "the child's two turns");
    assert_eq!(child[0].tools, [UPDATE_PLAN.name(), MEMORY_RECALL.name()]);
    assert_eq!(
        child[0].instructions, root[0].instructions,
        "the parent's system prompt"
    );
    let started = events(&capture, EVENT_CHILD_STARTED);
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].field("child_id"), Some("1"));
    assert_eq!(started[0].field("depth"), Some("1"));
    let ended = events(&capture, EVENT_CHILD_ENDED);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].field("status"), Some("done"));
    assert_eq!(ended[0].field("calls"), Some("2"));
}

/// The child's first call takes ten seconds of paused time: a wait of zero
/// milliseconds finds it not yet started, a wait of one second finds it in
/// that call, and the input sent then reaches its next turn.
#[tokio::test(start_paused = true)]
async fn test_spawn_wait_send_round_trip() {
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({"task": TASK}))],
        vec![call(
            "p2",
            WAIT_AGENT.name(),
            json!({CHILD_ID: 1, TIMEOUT_MS: 0}),
        )],
        vec![call(
            "p3",
            WAIT_AGENT.name(),
            json!({CHILD_ID: 1, TIMEOUT_MS: BRIEF_MS}),
        )],
        vec![call(
            "p4",
            SEND_INPUT.name(),
            json!({CHILD_ID: 1, "message": ALSO}),
        )],
        vec![call("p5", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![say(OK)],
    ])
    .with_child(
        TASK,
        [
            vec![call("c1", HTTP_REQUEST.name(), json!({}))],
            vec![say(CHILD_DONE)],
        ],
    );
    let mut tools = tools();
    tools.push(Slow::boxed(&HTTP_REQUEST, "fetched", SLOW_CALL));
    let engine = engine(tools, &script);
    let mut names = offered();
    names.push(HTTP_REQUEST.name());
    let lease = lease(&names, unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, OK);
    let root = requests_opening_with(&script, OPENING);
    assert_eq!(parsed(results(&root[1])[0]), json!({CHILD_ID: 1}));
    assert_eq!(
        parsed(results(&root[2])[1]),
        json!({STATUS: RUNNING}),
        "0 ms only looks"
    );
    assert_eq!(
        parsed(results(&root[3])[2]),
        json!({STATUS: RUNNING}),
        "mid-call"
    );
    assert_eq!(parsed(results(&root[4])[3]), json!({ACCEPTED: true}));
    assert_eq!(
        parsed(results(&root[5])[4]),
        json!({STATUS: DONE, ANSWER: CHILD_DONE})
    );
    let child = requests_opening_with(&script, TASK);
    assert_eq!(
        child[0].messages.first(),
        Some(&Message::User(TASK.to_owned())),
        "nothing was sent before the child's first turn"
    );
    assert_eq!(
        child[1].messages.last(),
        Some(&Message::User(ALSO.to_owned())),
        "the input reached the child's next turn"
    );
}

#[tokio::test]
async fn test_nested_depth_capped() {
    let script = Script::new([
        vec![call("p1", DELEGATE.name(), json!({"task": "d1"}))],
        vec![say(DONE)],
    ])
    .with_child(
        "d1",
        [
            vec![call("c1", DELEGATE.name(), json!({"task": "d2"}))],
            vec![say("d1 done")],
        ],
    )
    .with_child(
        "d2",
        [
            vec![call("c2", DELEGATE.name(), json!({"task": "d3"}))],
            vec![say("d2 done")],
        ],
    );
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let depth_one = requests_opening_with(&script, "d1");
    assert!(
        depth_one[0]
            .tools
            .iter()
            .any(|name| name == DELEGATE.name())
    );
    let depth_two = requests_opening_with(&script, "d2");
    let nested: Vec<&String> = depth_two[0]
        .tools
        .iter()
        .filter(|name| NESTED.iter().any(|entry| entry.name() == name.as_str()))
        .collect();
    assert!(nested.is_empty(), "offered at the cap: {nested:?}");
    assert_eq!(
        depth_two[0].tools,
        [UPDATE_PLAN.name(), MEMORY_RECALL.name()]
    );
    let refused = results(&depth_two[1])[0];
    assert!(
        refused.starts_with("[tool_not_offered]"),
        "a call past the cap is the router's to refuse: {refused}"
    );
    assert!(
        requests_opening_with(&script, "d3").is_empty(),
        "nothing started"
    );
}

#[tokio::test]
async fn test_child_usage_sums_into_report() {
    let script = Script::new([
        vec![
            call("p1", DELEGATE.name(), json!({"task": READS})),
            spent(10, 0, 5),
        ],
        vec![say(DONE), spent(0, 0, 0)],
    ])
    .with_child(
        READS,
        [
            vec![call("c1", UPDATE_PLAN.name(), json!({})), spent(12, 4, 2)],
            vec![say(SUMMARY), spent(8, 0, 4)],
        ],
    );
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.input_tokens, 30);
    assert_eq!(output.result.cached_input_tokens, 4);
    assert_eq!(output.result.output_tokens, 11);
}

/// Input sent before a child's first turn joins its task, so no provider
/// sees two user messages in a row.
#[tokio::test]
async fn test_input_before_the_first_turn_joins_the_task() {
    let script = Script::new([
        vec![call("p1", SPAWN.name(), json!({"task": TASK}))],
        vec![call(
            "p2",
            SEND_INPUT.name(),
            json!({CHILD_ID: 1, "message": ALSO}),
        )],
        vec![call("p3", WAIT_AGENT.name(), json!({CHILD_ID: 1}))],
        vec![say(OK)],
    ])
    .with_child(TASK, [vec![say(CHILD_DONE)]]);
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, OK);
    let child = requests_opening_with(&script, TASK);
    let users: Vec<&String> = child[0]
        .messages
        .iter()
        .filter_map(|message| match message {
            Message::User(text) => Some(text),
            Message::Assistant { .. } | Message::ToolResult { .. } => None,
        })
        .collect();
    assert_eq!(
        users,
        [&format!("{TASK}\n\n{ALSO}")],
        "{:?}",
        child[0].messages
    );
}
