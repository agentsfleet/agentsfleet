#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY};
use afr_providers::Message;
use afr_tools::Catalog;
use afr_tools::catalog::{MEMORY_FORGET, MEMORY_LIST, MEMORY_RECALL, MEMORY_STORE};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::Loop;
use crate::engine::{AgentEngine, AgentRun};
use crate::fixture::{Frames, Script, call, lease, say, unbounded};

/// What each call of the run read back, in call order.
fn read_back(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.as_str()),
            _other => None,
        })
        .collect()
}

#[tokio::test]
async fn test_memory_tools_round_trip_through_push() {
    let hydrated = [MemoryDelta {
        key: Cow::Borrowed("incident:41"),
        content: Cow::Borrowed("escalated to the on-call"),
        category: Cow::Borrowed(PINNED_CATEGORY),
    }];
    let script = Script::new([
        vec![call(
            "m1",
            MEMORY_STORE.name(),
            json!({"key": "incident:42", "content": "deploy 812 broke iad", "category": "daily"}),
        )],
        vec![call("m2", MEMORY_RECALL.name(), json!({"query": "incident"}))],
        vec![call("m3", MEMORY_FORGET.name(), json!({"key": "incident:41"}))],
        vec![call("m4", MEMORY_LIST.name(), json!({}))],
        vec![say("incident 42 is new")],
    ]);
    let engine = Loop::new(Catalog::hosted(), script.replay());
    let names = [MEMORY_STORE, MEMORY_RECALL, MEMORY_FORGET, MEMORY_LIST].map(|entry| entry.name());
    let lease = lease(&names, unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: &hydrated,
            executor: None,
            events: &sink,
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    frames.taken();

    let sent = script.sent();
    let read = read_back(&sent[4].messages);
    assert_eq!(read[0], "stored incident:42");
    assert_eq!(
        read[1],
        "incident:42 (daily): deploy 812 broke iad\nincident:41 (core): escalated to the on-call",
        "a store is recalled in the same run, ahead of what was hydrated"
    );
    assert!(read[2].starts_with("incident:41 is forgotten"), "{}", read[2]);
    assert_eq!(read[3], "incident:42 (daily)", "a forget holds for the rest of the run");
    assert_eq!(
        output.memory,
        [MemoryDelta {
            key: Cow::Borrowed("incident:42"),
            content: Cow::Borrowed("deploy 812 broke iad"),
            category: Cow::Borrowed("daily"),
        }],
        "the push carries what the run stored, and nothing it only read"
    );
}
