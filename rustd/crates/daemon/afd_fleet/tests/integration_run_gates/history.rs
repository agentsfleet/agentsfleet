//! A chat run's earlier turns, as the plane hands them to a runner.
//!
//! `integration_run_history.rs` proves the read; these drive the path around
//! it: `RunService::claim_selected` issues a run, `RunService::report` ends it, and the
//! next run's payload carries what that report left. They live under this
//! suite because an issued run needs the provider seed only it owns.

use std::sync::Arc;
use std::time::Duration;

use af_wire::event::EventType;
use afd_db::test_util::unreachable_db;
use afd_fleet::run::RunService;
use afd_turns::view::History;

use super::chat::{ASKED_LATER, Chat, FIRST_ANSWER, FIRST_MESSAGE, FOLLOW_UP, first_turn};
use crate::report_seed::SLICE;

/// A chat message held after the fleet answered the one before it carries
/// that message and answer in its run's `history`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_follow_up_run_carries_the_previous_turn() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let asked = chat
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let follow_up = chat.run(&chat.runner, chat.fixtures.after(SLICE * 2)).await;

    assert_eq!(follow_up.event_id, asked);
    assert_eq!(follow_up.history, first_turn());
    chat.fixtures.cleanup().await;
}

/// Two messages queued at once: the second waits while the first holds the
/// fleet's slot, then runs after the first's report and reads it as a
/// finished turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_second_message_waits_for_the_first_and_carries_it() {
    let chat = Chat::open().await;
    let asked = chat
        .ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    let second = chat
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let first = chat.run(&chat.runner, chat.fixtures.enrolled_at()).await;
    let waiting = chat.claim(&chat.spare).await;
    chat.answer(
        &chat.runner,
        &first,
        FIRST_ANSWER,
        chat.fixtures.after(SLICE),
    )
    .await;
    let follow_up = chat.run(&chat.spare, chat.fixtures.after(SLICE * 2)).await;

    assert_eq!(first.event_id, asked);
    assert!(
        first.history.is_empty(),
        "nothing finished before the first"
    );
    assert!(waiting.is_none(), "one fleet, one holder: the second waits");
    assert_eq!(follow_up.event_id, second);
    assert_eq!(follow_up.history, first_turn());
    chat.fixtures.cleanup().await;
}

/// A webhook held after a finished chat turn carries no history: turns are
/// for chat runs alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_non_chat_run_carries_no_history_through_the_plane() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let delivered = chat
        .ask(
            EventType::Webhook,
            "a push landed",
            chat.fixtures.after(ASKED_LATER),
        )
        .await;

    let webhook = chat.run(&chat.runner, chat.fixtures.after(SLICE * 2)).await;

    assert_eq!(webhook.event_id, delivered);
    assert!(webhook.history.is_empty(), "{:?}", webhook.history);
    chat.fixtures.cleanup().await;
}

/// A thread that will not answer costs a follow-up its turns, never its
/// run: the plane issues it with no history rather than refusing the poll.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_follow_up_run_is_issued_when_its_thread_will_not_answer() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let asked = chat
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;
    let silent = RunService {
        task_reports: None,
        thread: Arc::new(History::new(unreachable_db())),
        ..chat.fixtures.plane()
    };

    let follow_up = chat
        .run_through(&silent, &chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

    assert_eq!(follow_up.event_id, asked);
    assert!(follow_up.history.is_empty(), "{:?}", follow_up.history);
    chat.fixtures.cleanup().await;
}

/// A run reads the turns before its own event: a run that finished after
/// the held event was stamped is no turn of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_run_history_stops_at_its_own_event() {
    let chat = Chat::open().await;
    let asked = chat
        .ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    chat.finished(FOLLOW_UP, FIRST_ANSWER, chat.fixtures.after(SLICE))
        .await;

    let held = chat.run(&chat.runner, chat.fixtures.after(SLICE * 2)).await;

    assert_eq!(held.event_id, asked);
    assert!(held.history.is_empty(), "{:?}", held.history);
    chat.fixtures.cleanup().await;
}

/// The issued-run line carries how long its event waited for a runner, on
/// the daemon's own clock: admitted at T, polled at T + 350 ms, logged 350
/// plus the time the poll itself took to write the run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_runner_run_issued_logs_queue_wait() {
    const WAITED_MS: i64 = 350;
    const WAITED: Duration = Duration::from_millis(WAITED_MS.unsigned_abs());
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;

    let log = af_core::test_util::trace::Capture::install();
    let polled = std::time::Instant::now();
    chat.run(&chat.runner, chat.fixtures.after(WAITED)).await;
    let call_ms = i64::try_from(polled.elapsed().as_millis()).expect("a short call");
    let issued = log.only("runner_run_issued");
    drop(log);

    let logged: i64 = issued
        .field("queue_wait_ms")
        .and_then(|millis| millis.parse().ok())
        .expect("a fresh run logs its queue wait");
    assert!(
        (WAITED_MS..=WAITED_MS + call_ms).contains(&logged),
        "{logged} ms outside {WAITED_MS}..={} ms: {:?}",
        WAITED_MS + call_ms,
        issued.fields
    );
    chat.fixtures.cleanup().await;
}
