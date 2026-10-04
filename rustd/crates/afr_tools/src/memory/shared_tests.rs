//! Dimensions 3.2, 4.3 and 4.4: what the memory tools do with the
//! workspace's shared memory.

use std::borrow::Cow;

use afd_wire::memory::SharedMemory;
use afr_memory::{Hydrated, Seed};
use serde_json::json;

use crate::handler::Typed;
use crate::lease::Lease;
use crate::memory::{MemoryForget, MemoryRecall, MemoryStore};
use crate::runtime::ToolErrorCode;
use crate::testing::call;

/// The fleet that published the shared entry these tests read.
const WRITER: &str = "ticket-3";

/// One entry another fleet published, as the hydrate reply carries it.
fn published() -> Vec<SharedMemory<'static>> {
    vec![SharedMemory {
        key: Cow::Borrowed("deploy_target"),
        content: Cow::Borrowed("deploy 812 broke iad"),
        category: Cow::Borrowed("core"),
        writer_fleet_id: Cow::Borrowed("01990000-0000-7000-8000-0000000000c3"),
        writer_fleet_name: Cow::Borrowed(WRITER),
        updated_at: 1_760_000_000_000,
    }]
}

#[tokio::test]
async fn test_workspace_store_needs_publish() {
    let mut lease = Lease::new(
        Box::new(Hydrated::new(Seed {
            publish: false,
            ..Seed::default()
        })),
        afr_egress::testing::closed(),
    );

    let refused = call(
        Typed::boxed(MemoryStore).as_ref(),
        &mut lease,
        json!({"key": "deploy_target", "content": "iad", "visibility": "workspace"}),
    )
    .await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::WorkspaceMemoryNotGranted)
    );
    assert!(
        refused.text.starts_with("[workspace_memory_not_granted]"),
        "{}",
        refused.text
    );
    assert!(lease.memory.into_pending().is_empty(), "nothing is pending");
}

#[tokio::test]
async fn a_publisher_stores_a_share_and_it_is_pushed_as_one() {
    let mut lease = Lease::new(
        Box::new(Hydrated::new(Seed {
            publish: true,
            ..Seed::default()
        })),
        afr_egress::testing::closed(),
    );

    let stored = call(
        Typed::boxed(MemoryStore).as_ref(),
        &mut lease,
        json!({"key": "deploy_target", "content": "iad", "visibility": "workspace"}),
    )
    .await;

    assert_eq!(stored.error_code, None, "{}", stored.text);
    let pushed = lease.memory.into_pending();
    assert!(pushed.iter().all(|delta| delta.visibility.is_workspace()));
}

#[tokio::test]
async fn test_recall_names_the_writer_of_a_shared_entry() {
    let shared = published();
    let mut lease = Lease::new(
        Box::new(Hydrated::new(Seed {
            shared: &shared,
            ..Seed::default()
        })),
        afr_egress::testing::closed(),
    );

    let recalled = call(
        Typed::boxed(MemoryRecall).as_ref(),
        &mut lease,
        json!({"query": "deploy"}),
    )
    .await;

    assert_eq!(
        recalled.text,
        format!("deploy_target (core, shared by {WRITER}): deploy 812 broke iad")
    );
}

#[tokio::test]
async fn test_forget_leaves_another_fleets_entry() {
    let shared = published();
    let mut lease = Lease::new(
        Box::new(Hydrated::new(Seed {
            shared: &shared,
            ..Seed::default()
        })),
        afr_egress::testing::closed(),
    );

    let forgot = call(
        Typed::boxed(MemoryForget).as_ref(),
        &mut lease,
        json!({"key": "deploy_target"}),
    )
    .await;
    assert_eq!(forgot.text, "nothing remembered under deploy_target");

    let recalled = call(
        Typed::boxed(MemoryRecall).as_ref(),
        &mut lease,
        json!({"query": "deploy"}),
    )
    .await;
    assert!(
        recalled.text.contains(WRITER),
        "the shared entry is kept: {}",
        recalled.text
    );
}
