//! A lease's memory window, as the plane hands it to a runner.
//!
//! The window is read after the issue, under the fleet's slot, so it holds
//! what the fleet's last report wrote; a read that fails costs the lease its
//! window, never the lease.

use std::borrow::Cow;

use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::unreachable_db;
use afd_fleet::lease::Plane;
use afd_memory::Memories;
use afd_wire::event::EventType;
use afd_wire::memory::{MemoryDelta, Visibility};

use super::chat::{ASKED_LATER, Chat, FIRST_ANSWER, FIRST_MESSAGE, FOLLOW_UP};
use crate::report_seed::SLICE;

/// Two things the first run learned.
const LEARNED: [(&str, &str); 2] = [
    ("flaky-test", "b fails only under load"),
    ("owner", "the ci channel"),
];

/// [`LEARNED`] as the deltas a report carries.
fn learned() -> Vec<MemoryDelta<'static>> {
    LEARNED
        .iter()
        .map(|(key, content)| MemoryDelta {
            key: Cow::Borrowed(key),
            content: Cow::Borrowed(content),
            category: Cow::Borrowed("core"),
            visibility: Visibility::Fleet,
        })
        .collect()
}

/// [`LEARNED`] as a window carries it, newest first.
fn remembered() -> Vec<(String, String)> {
    let mut window: Vec<(String, String)> = LEARNED
        .iter()
        .map(|(key, content)| ((*key).to_owned(), (*content).to_owned()))
        .collect();
    window.sort();
    window
}

/// A fresh fleet's lease carries an empty window, and the lease after a
/// report that remembered two things carries both: no memory read needed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_carries_the_memory_window() {
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    let first = chat.lease(&chat.runner, chat.fixtures.enrolled_at()).await;
    chat.answer_remembering(
        &chat.runner,
        &first,
        FIRST_ANSWER,
        learned(),
        chat.fixtures.after(SLICE),
    )
    .await;
    chat.ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let follow_up = chat
        .lease(&chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

    let mut carried = follow_up.memory.clone().unwrap_or_default();
    carried.sort();
    assert_eq!(
        (first.memory, follow_up.memory.is_some(), carried),
        (Some(Vec::new()), true, remembered())
    );
    chat.fixtures.cleanup().await;
}

/// A second message waits while the first holds the fleet's slot, and the
/// lease it gets after the first's report carries what that report wrote.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_window_follows_the_slot() {
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    chat.ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let first = chat.lease(&chat.runner, chat.fixtures.enrolled_at()).await;
    let waiting = chat.claim(&chat.spare).await;
    chat.answer_remembering(
        &chat.runner,
        &first,
        FIRST_ANSWER,
        learned(),
        chat.fixtures.after(SLICE),
    )
    .await;
    let follow_up = chat
        .lease(&chat.spare, chat.fixtures.after(SLICE * 2))
        .await;

    let mut carried = follow_up.memory.unwrap_or_default();
    carried.sort();
    assert_eq!((waiting.is_none(), carried), (true, remembered()));
    chat.fixtures.cleanup().await;
}

/// A memory store that will not answer costs the lease its window, never the
/// lease: it issues with none, and the read's failure is logged once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_window_read_failure_issues_without_a_window() {
    let chat = Chat::open().await;
    let asked = chat
        .ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    let forgetful = Plane {
        memories: Memories::new(unreachable_db(), Entropy::new()),
        ..chat.fixtures.plane()
    };

    let log = Capture::install();
    let leased = chat
        .lease_through(&forgetful, &chat.runner, chat.fixtures.enrolled_at())
        .await;
    let failed = log.only("lease_window_read_failed");
    drop(log);

    assert_eq!(
        (
            leased.event_id,
            leased.memory,
            failed.field("fleet_id").is_some()
        ),
        (asked, None, true)
    );
    chat.fixtures.cleanup().await;
}
