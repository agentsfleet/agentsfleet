//! What a child is refused: the run's caps, and a tool its parent lacks.

#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::Chunk;
use afr_tools::catalog::{DELEGATE, MEMORY_RECALL, SPAWN, UPDATE_PLAN, WAIT_AGENT};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::child::EVENT_CHILD_STARTED;
use super::fixture::{
    CHILD_ID, DONE, OPENING, READS, SUMMARY, events, offered, parsed, requests_opening_with,
    results, tools,
};
use super::start::EVENT_CHILD_REFUSED;
use crate::fixture::{Script, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

#[tokio::test]
async fn test_children_caps_refuse() {
    let capture = Capture::install();
    let spawns: Vec<Chunk> = (1..=5)
        .map(|n| {
            call(
                &format!("s{n}"),
                SPAWN.name(),
                json!({"task": format!("spawned {n}")}),
            )
        })
        .collect();
    let waits: Vec<Chunk> = (1..=4)
        .map(|n| call(&format!("w{n}"), WAIT_AGENT.name(), json!({CHILD_ID: n})))
        .collect();
    let delegates: Vec<Chunk> = (1..=13)
        .map(|n| {
            call(
                &format!("d{n}"),
                DELEGATE.name(),
                json!({"task": format!("delegated {n}")}),
            )
        })
        .collect();
    let mut script = Script::new([spawns, waits, delegates, vec![say(DONE)]]);
    for n in 1..=4 {
        script = script.with_child(&format!("spawned {n}"), [vec![say("x")]]);
    }
    for n in 1..=13 {
        script = script.with_child(&format!("delegated {n}"), [vec![say("x")]]);
    }
    let engine = engine(tools(), &script);
    let lease = lease(&offered(), unbounded());

    let (output, _frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, DONE);
    let root = requests_opening_with(&script, OPENING);
    let spawned = results(&root[1]);
    assert_eq!(parsed(spawned[3]), json!({CHILD_ID: 4}));
    assert!(
        spawned[4].starts_with("[child_cap_reached]"),
        "{}",
        spawned[4]
    );
    // The conversation keeps every result: five spawns, four waits, then
    // the thirteen delegates.
    let all = results(&root[3]);
    let from_delegates = &all[9..];
    assert_eq!(from_delegates.len(), 13);
    assert_eq!(from_delegates[11], "x", "the sixteenth child started");
    assert!(
        from_delegates[12].starts_with("[child_cap_reached]"),
        "{}",
        from_delegates[12]
    );
    assert!(requests_opening_with(&script, "spawned 5").is_empty());
    assert!(requests_opening_with(&script, "delegated 13").is_empty());
    let refused = events(&capture, EVENT_CHILD_REFUSED);
    assert_eq!(refused.len(), 2);
    assert!(
        refused
            .iter()
            .all(|event| event.field("error_code") == Some("child_cap_reached"))
    );
}

#[tokio::test]
async fn test_child_tools_subset_of_parent() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call(
            "p1",
            DELEGATE.name(),
            json!({"task": READS, "tools": [UPDATE_PLAN.name(), MEMORY_RECALL.name()]}),
        )],
        vec![say(DONE)],
    ])
    .with_child(READS, [vec![say(SUMMARY)]]);
    let engine = engine(tools(), &script);
    let lease = lease(&[UPDATE_PLAN.name(), DELEGATE.name()], unbounded());

    let (_output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    let root = requests_opening_with(&script, OPENING);
    let refused = results(&root[1])[0];
    assert!(refused.starts_with("[child_tool_not_held]"), "{refused}");
    assert!(
        refused.contains(MEMORY_RECALL.name()),
        "names the tool: {refused}"
    );
    assert!(
        requests_opening_with(&script, READS).is_empty(),
        "nothing started"
    );
    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Failed)]
    );
    let logged = events(&capture, EVENT_CHILD_REFUSED);
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].field("error_code"), Some("child_tool_not_held"));
    assert!(events(&capture, EVENT_CHILD_STARTED).is_empty());
}
