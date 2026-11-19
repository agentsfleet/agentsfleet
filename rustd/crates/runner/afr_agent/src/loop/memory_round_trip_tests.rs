//! The memory tool round trip preserves the pending push and locally visible state.

use super::Loop;
use crate::{
    engine::{AgentEngine, AgentRun, Meter, RunOutput},
    fixture::{Frames, Script, call, lease, say, unbounded},
    testing::Discard,
};
use af_egress::testing::{CountingMint, RecordingTransport};
use af_providers::Message;
use af_wire::memory::{MemoryDelta, PINNED_CATEGORY};
use afr_tools::{
    Catalog,
    catalog::{MEMORY_FORGET, MEMORY_LIST, MEMORY_RECALL, MEMORY_STORE},
};
use serde_json::json;
use std::{borrow::Cow, sync::Arc};
use tokio_util::sync::CancellationToken;

type Checked = Result<(), Box<dyn std::error::Error + Send + Sync>>;

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
async fn test_memory_tools_round_trip_through_push() -> Checked {
    let hydrated = [MemoryDelta {
        key: Cow::Borrowed("incident:41"),
        content: Cow::Borrowed("escalated to the on-call"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: af_wire::memory::Visibility::Fleet,
    }];
    let script = memory_script();
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let names = [MEMORY_STORE, MEMORY_RECALL, MEMORY_FORGET, MEMORY_LIST].map(|entry| entry.name());
    let lease = lease(&names, unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            recovery: None,
            recovered_task: None,
            recovered_turn: None,
            prepared_task: None,
            lease: &lease,
            memory: afr_memory::Seed::window(&hydrated),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            inbox: &crate::testing::NoInbox,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await?;
    frames.taken();

    assert_round_trip(&script, &output)?;
    Ok(())
}

fn memory_script() -> Script {
    Script::new([
        vec![call(
            "m1",
            MEMORY_STORE.name(),
            json!({"key": "incident:42", "content": "deploy 812 broke iad", "category": "daily"}),
        )],
        vec![call(
            "m2",
            MEMORY_RECALL.name(),
            json!({"query": "incident"}),
        )],
        vec![call(
            "m3",
            MEMORY_FORGET.name(),
            json!({"key": "incident:41"}),
        )],
        vec![call("m4", MEMORY_LIST.name(), json!({}))],
        vec![say("incident 42 is new")],
    ])
}

fn assert_round_trip(script: &Script, output: &RunOutput) -> Checked {
    let sent = script.sent();
    let request = sent.get(4).ok_or("fifth model request missing")?;
    let read = read_back(&request.messages);
    let [stored, recalled, forgotten, listed] = read.as_slice() else {
        return Err("four memory tool results required".into());
    };
    assert_eq!(*stored, "stored incident:42");
    assert_eq!(
        *recalled,
        "incident:42 (daily): deploy 812 broke iad\nincident:41 (core): escalated to the on-call",
        "a store is recalled in the same run, ahead of what was hydrated"
    );
    assert!(
        forgotten.starts_with("incident:41 is forgotten"),
        "{}",
        forgotten
    );
    assert_eq!(
        *listed, "incident:42 (daily)",
        "a forget holds for the rest of the run"
    );
    assert_eq!(
        output.memory,
        [MemoryDelta {
            key: Cow::Borrowed("incident:42"),
            content: Cow::Borrowed("deploy 812 broke iad"),
            category: Cow::Borrowed("daily"),
            visibility: af_wire::memory::Visibility::Fleet,
        }],
        "the push carries what the run stored, and nothing it only read"
    );
    Ok(())
}
