#![expect(
    clippy::indexing_slicing,
    reason = "test module: a missing key or element should fail the test loudly"
)]

use std::borrow::Cow;

use af_wire::memory::{
    MAX_CONTENT_LEN, MAX_PUSH_BYTES, MemoryDelta, PINNED_CATEGORY, RECALL_LIMIT_MAX, Visibility,
};
use afr_memory::{Loaded, Seed};
use serde_json::json;

use crate::run::Run;
use crate::runtime::ToolErrorCode;
use crate::testing::call_built;

fn loaded() -> Vec<MemoryDelta<'static>> {
    vec![MemoryDelta {
        key: Cow::Borrowed("operator_context:codename"),
        content: Cow::Borrowed("bluebird"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Fleet,
    }]
}

#[tokio::test]
async fn the_four_tools_share_one_run_memory() {
    let window = loaded();
    let run = Run::new(
        Loaded::new(Seed::window(&window)),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let stored = call_built(
        &run,
        "memory_store",
        json!({"key": "operator_context:review_status", "content": "two findings open"}),
    )
    .await;
    assert_eq!(stored.text, "stored operator_context:review_status");

    let recalled = call_built(&run, "memory_recall", json!({"query": "OPERATOR_context"})).await;
    assert_eq!(
        recalled.text,
        "operator_context:review_status (core): two findings open\n\
         operator_context:codename (core): bluebird"
    );

    let dropped = call_built(
        &run,
        "memory_forget",
        json!({"key": "operator_context:codename"}),
    )
    .await;
    assert!(
        dropped
            .text
            .starts_with("operator_context:codename is forgotten"),
        "{}",
        dropped.text
    );
    let listed = call_built(&run, "memory_list", json!({})).await;
    assert_eq!(listed.text, "operator_context:review_status (core)");

    let pushed = run.memory.into_pending();
    assert_eq!(pushed.len(), 1);
    assert_eq!(
        pushed[0].category, PINNED_CATEGORY,
        "no category names the pinned one"
    );
}

#[tokio::test]
async fn empty_answers_say_what_was_looked_for() {
    let run = Run::default();

    let recalled = call_built(&run, "memory_recall", json!({"query": "deploy"})).await;
    assert_eq!(recalled.text, "nothing remembered matches deploy");
    let listed = call_built(&run, "memory_list", json!({"category": "daily"})).await;
    assert_eq!(listed.text, "nothing remembered");
    let forgot = call_built(&run, "memory_forget", json!({"key": "ghost"})).await;
    assert_eq!(forgot.text, "nothing remembered under ghost");
}

#[tokio::test]
async fn a_store_the_daemon_would_skip_is_refused_with_its_code() {
    let run = Run::default();

    let empty = call_built(&run, "memory_store", json!({"key": "", "content": "c"})).await;
    assert_eq!(
        empty.error_code,
        Some(ToolErrorCode::InvalidArguments.into())
    );
    assert!(
        empty.text.starts_with("[invalid_arguments] key"),
        "{}",
        empty.text
    );

    let content = "c".repeat(MAX_CONTENT_LEN);
    let mut full = None;
    for at in 0..=(MAX_PUSH_BYTES / MAX_CONTENT_LEN) {
        let output = call_built(
            &run,
            "memory_store",
            json!({"key": format!("k{at}"), "content": content}),
        )
        .await;
        if output.error_code.is_some() {
            full = Some(output);
            break;
        }
    }
    let full = full.map(|output| output.error_code);
    assert_eq!(full, Some(Some(ToolErrorCode::MemoryFull.into())));
}

#[tokio::test]
async fn recall_never_answers_more_than_its_ceiling() {
    let run = Run::default();
    for at in 0..RECALL_LIMIT_MAX + 10 {
        call_built(
            &run,
            "memory_store",
            json!({"key": format!("k{at}"), "content": "c"}),
        )
        .await;
    }

    let recalled = call_built(&run, "memory_recall", json!({"query": "k", "limit": 500})).await;
    assert_eq!(recalled.text.lines().count(), RECALL_LIMIT_MAX);
    let defaulted = call_built(&run, "memory_recall", json!({"query": "k"})).await;
    assert_eq!(defaulted.text.lines().count(), 5);
}
