#![expect(
    clippy::indexing_slicing,
    reason = "test module: a missing key or element should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::memory::{MAX_CONTENT_LEN, MAX_PUSH_BYTES, MemoryDelta, PINNED_CATEGORY};
use afr_memory::Memory;
use serde_json::json;

use crate::handler::Typed;
use crate::lease::Lease;
use crate::memory::{MemoryForget, MemoryList, MemoryRecall, MemoryStore};
use crate::runtime::ToolErrorCode;
use crate::testing::call;

fn hydrated() -> Vec<MemoryDelta<'static>> {
    vec![MemoryDelta {
        key: Cow::Borrowed("operator_context:codename"),
        content: Cow::Borrowed("bluebird"),
        category: Cow::Borrowed(PINNED_CATEGORY),
    }]
}

#[tokio::test]
async fn the_four_tools_share_one_lease_memory() {
    let window = hydrated();
    let mut lease = Lease::new(Memory::hydrated(&window));
    let (store, recall, list, forget) = (
        Typed::boxed(MemoryStore),
        Typed::boxed(MemoryRecall),
        Typed::boxed(MemoryList),
        Typed::boxed(MemoryForget),
    );

    let stored = call(
        store.as_ref(),
        &mut lease,
        json!({"key": "operator_context:review_status", "content": "two findings open"}),
    )
    .await;
    assert_eq!(stored.text, "stored operator_context:review_status");

    let recalled = call(recall.as_ref(), &mut lease, json!({"query": "OPERATOR_context"})).await;
    assert_eq!(
        recalled.text,
        "operator_context:review_status (core): two findings open\n\
         operator_context:codename (core): bluebird"
    );

    let dropped = call(forget.as_ref(), &mut lease, json!({"key": "operator_context:codename"})).await;
    assert!(dropped.text.starts_with("operator_context:codename is forgotten"), "{}", dropped.text);
    let listed = call(list.as_ref(), &mut lease, json!({})).await;
    assert_eq!(listed.text, "operator_context:review_status (core)");

    let pushed = lease.memory.into_stored();
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0].category, PINNED_CATEGORY, "no category names the pinned one");
}

#[tokio::test]
async fn empty_answers_say_what_was_looked_for() {
    let mut lease = Lease::default();

    let recalled = call(Typed::boxed(MemoryRecall).as_ref(), &mut lease, json!({"query": "deploy"})).await;
    assert_eq!(recalled.text, "nothing remembered matches deploy");
    let listed = call(Typed::boxed(MemoryList).as_ref(), &mut lease, json!({"category": "daily"})).await;
    assert_eq!(listed.text, "nothing remembered");
    let forgot = call(Typed::boxed(MemoryForget).as_ref(), &mut lease, json!({"key": "ghost"})).await;
    assert_eq!(forgot.text, "nothing remembered under ghost");
}

#[tokio::test]
async fn a_store_the_daemon_would_skip_is_refused_with_its_code() {
    let store = Typed::boxed(MemoryStore);
    let mut lease = Lease::default();

    let empty = call(store.as_ref(), &mut lease, json!({"key": "", "content": "c"})).await;
    assert_eq!(empty.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(empty.text.starts_with("[invalid_arguments] key"), "{}", empty.text);

    let content = "c".repeat(MAX_CONTENT_LEN);
    let mut full = None;
    for at in 0..=(MAX_PUSH_BYTES / MAX_CONTENT_LEN) {
        let output = call(store.as_ref(), &mut lease, json!({"key": format!("k{at}"), "content": content})).await;
        if output.error_code.is_some() {
            full = Some(output);
            break;
        }
    }
    let full = full.map(|output| output.error_code);
    assert_eq!(full, Some(Some(ToolErrorCode::MemoryFull)));
}

#[tokio::test]
async fn recall_never_answers_more_than_its_ceiling() {
    let mut lease = Lease::default();
    let store = Typed::boxed(MemoryStore);
    for at in 0..60 {
        call(store.as_ref(), &mut lease, json!({"key": format!("k{at}"), "content": "c"})).await;
    }

    let recalled = call(Typed::boxed(MemoryRecall).as_ref(), &mut lease, json!({"query": "k", "limit": 500})).await;
    assert_eq!(recalled.text.lines().count(), 50);
    let defaulted = call(Typed::boxed(MemoryRecall).as_ref(), &mut lease, json!({"query": "k"})).await;
    assert_eq!(defaulted.text.lines().count(), 5);
}
