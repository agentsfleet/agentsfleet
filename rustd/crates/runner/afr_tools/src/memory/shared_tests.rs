//! What the memory tools do with the workspace's shared memory.

use std::borrow::Cow;

use af_wire::memory::SharedMemory;
use afr_memory::{Loaded, Seed};
use serde_json::json;

use crate::run::Run;
use crate::runtime::ToolErrorCode;
use crate::testing::call_built;

/// The fleet that published the shared entry these tests read.
const WRITER: &str = "ticket-3";

/// One entry another fleet published, as the load reply carries it.
fn published() -> Vec<SharedMemory<'static>> {
    vec![SharedMemory {
        key: Cow::Borrowed("deploy_target"),
        content: Cow::Borrowed("deploy 812 broke iad"),
        category: Cow::Borrowed("core"),
        writer_fleet_id: Cow::Borrowed("01990000-0000-7000-8000-0000000000c3"),
        writer_fleet_name: Cow::Borrowed(WRITER),
        updated_at: jiff::Timestamp::constant(1_760_000_000, 0),
    }]
}

#[tokio::test]
async fn a_shared_store_needs_the_publish_grant() {
    let run = Run::new(
        Loaded::new(Seed {
            publish: false,
            ..Seed::default()
        }),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let refused = call_built(
        &run,
        "memory_store",
        json!({"key": "deploy_target", "content": "iad", "visibility": "shared"}),
    )
    .await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SharedMemoryNotGranted.into())
    );
    assert!(
        refused.text.starts_with("[shared_memory_not_granted]"),
        "{}",
        refused.text
    );
    assert!(run.memory.into_pending().is_empty(), "nothing is pending");
}

#[tokio::test]
async fn a_publisher_stores_a_share_and_it_is_pushed_as_one() {
    let run = Run::new(
        Loaded::new(Seed {
            publish: true,
            ..Seed::default()
        }),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let stored = call_built(
        &run,
        "memory_store",
        json!({"key": "deploy_target", "content": "iad", "visibility": "shared"}),
    )
    .await;

    assert_eq!(stored.error_code, None, "{}", stored.text);
    let pushed = run.memory.into_pending();
    assert_eq!(pushed.len(), 1, "one store is one pending push");
    assert!(pushed.iter().all(|delta| delta.visibility.is_workspace()));
}

#[tokio::test]
async fn test_recall_names_the_writer_of_a_shared_entry() {
    let shared = published();
    let run = Run::new(
        Loaded::new(Seed {
            shared: &shared,
            ..Seed::default()
        }),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let recalled = call_built(&run, "memory_recall", json!({"query": "deploy"})).await;

    assert_eq!(
        recalled.text,
        format!("deploy_target (core, shared by {WRITER}): deploy 812 broke iad")
    );
}

#[tokio::test]
async fn test_forget_leaves_another_fleets_entry() {
    let shared = published();
    let run = Run::new(
        Loaded::new(Seed {
            shared: &shared,
            ..Seed::default()
        }),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let forgot = call_built(&run, "memory_forget", json!({"key": "deploy_target"})).await;
    assert_eq!(forgot.text, "nothing remembered under deploy_target");

    let recalled = call_built(&run, "memory_recall", json!({"query": "deploy"})).await;
    assert!(
        recalled.text.contains(WRITER),
        "the shared entry is kept: {}",
        recalled.text
    );
}

#[tokio::test]
async fn a_publisher_naming_private_reach_keeps_the_entry_to_itself() {
    let run = Run::new(
        Loaded::new(Seed {
            publish: true,
            ..Seed::default()
        }),
        af_egress::testing::closed(),
        &crate::verbs::CLOSED,
    );

    let stored = call_built(
        &run,
        "memory_store",
        json!({"key": "deploy_target", "content": "iad", "visibility": "private"}),
    )
    .await;

    assert_eq!(stored.error_code, None, "{}", stored.text);
    let pushed = run.memory.into_pending();
    assert_eq!(pushed.len(), 1);
    assert!(
        pushed.iter().all(|delta| !delta.visibility.is_workspace()),
        "the grant to publish is not a choice to"
    );
}
