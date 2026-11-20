//! Which messages an assignment answers, in which order, and to whom.

use afd_turns::{Producer, Reply};

use super::support::{
    Checked, STEER_JSON, STEER_TEXT, WEBHOOK_JSON, assign, assign_as_holder, park, send, send_as,
    stamp_assignment, workspace_of,
};
use crate::report_seed::held;

/// A person's later steers in the run's conversation fold, oldest first, as
/// the text the model would read had they started the run; the send reads as
/// one that folds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_live_run_is_assigned_a_persons_messages_oldest_first() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let first = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let second = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let live = second.run.live_run.as_ref().ok_or("the held run is live")?;
    assert!(
        !live.durable_task && live.same_conversation && !second.run.earlier_waiting,
        "the send folds: {:?}",
        second.run
    );
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [first.assigned(STEER_TEXT), second.assigned(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A message no person sent is never folded and never passed: a webhook, a
/// gate's continuation or a machine's steer runs as a run of its own, and a
/// person's steer behind it waits, and reads as waiting.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_message_no_person_sent_stops_the_fold() -> Checked {
    let machine = afd_turns::view::ACTOR_MACHINE;
    for (producer, actor) in [
        (Producer::Webhook, crate::seed::ACTOR),
        (Producer::GateContinuation, crate::seed::ACTOR),
        (Producer::Steer, machine),
    ] {
        let held = held().await;
        let workspace = workspace_of(&held).await?;
        let before = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
        let barrier = send_as(
            &held,
            &workspace,
            producer,
            actor,
            WEBHOOK_JSON,
            Reply::None,
        );
        let barrier = barrier.await?;
        let behind = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
        assert!(
            behind.run.earlier_waiting,
            "{producer:?} by {actor} waits ahead of this send"
        );
        assert_eq!(
            assign_as_holder(&held, 0).await?,
            [before.assigned(STEER_TEXT)],
            "{producer:?} by {actor} (seq {}) is neither folded nor passed",
            barrier.sequence
        );
        held.fixtures.cleanup().await;
    }
    Ok(())
}

/// A parked message is never passed: nothing after it is assigned, and a
/// send behind it answers that it waits.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_parked_message_stops_the_fold() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let before = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let gated = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    park(&held, &gated.id).await?;
    let behind = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(
        behind.run.earlier_waiting,
        "the parked message waits ahead of this send"
    );
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [before.assigned(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// An assignment never replaces a stamp a later fence wrote: the message stays
/// with the run that assigned it last.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_never_replaces_a_later_fences_stamp() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let later = i64::try_from(held.fence.as_u64())? + 1;
    stamp_assignment(&held, &sent.id, later).await?;
    assert_eq!(assign_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// Only the runner holding the run, under its current fence, is assigned any.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runner_or_a_superseded_fence_is_assigned_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let fence = held.fence.as_u64();
    assert_eq!(assign(&held, &held.spare, fence, 0).await?, Vec::new());
    assert_eq!(
        assign(&held, &held.runner, fence + 1, 0).await?,
        Vec::new(),
        "a fence the thread never issued is assigned nothing"
    );
    assert_eq!(
        assign(&held, &held.runner, fence, 0).await?,
        [sent.assigned(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// An assignment whose answer was lost reads the same messages again, and the
/// cursor moves past what the run read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_repeated_assignment_answers_the_same_messages() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let first = assign_as_holder(&held, 0).await?;
    assert_eq!(first, [sent.assigned(STEER_TEXT)]);
    assert_eq!(assign_as_holder(&held, 0).await?, first);
    assert_eq!(assign_as_holder(&held, sent.sequence).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// An internal setup event is no turn, so it cannot own folds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_internal_setup_event_is_not_a_turn_head() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let before = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(before.run.live_run.is_some());
    // Moves the run to a history-only copy of its event: a turn written opened and
    // settled at birth, with no producer.
    let changed = sqlx::query(
        "WITH moved AS (INSERT INTO thread.turns \
           (id, thread_id, workspace_id, actor, kind, status, request_json, \
            opened_at, settled_at, created_at, updated_at) \
         SELECT $2, e.thread_id, e.workspace_id, e.actor, e.kind, e.status, e.request_json, \
                now(), now(), now(), now() \
         FROM thread.runs l JOIN thread.turns e ON e.id = l.turn_id \
         WHERE l.id = $1 RETURNING id) \
         UPDATE thread.runs SET turn_id = $2 WHERE id = $1 AND EXISTS (SELECT 1 FROM moved)",
    )
    .bind(&held.run_key)
    .bind(afd_db::test_util::mint_id())
    .execute(&mut *held.fixtures.database.acquire().await?)
    .await?;
    assert_eq!(changed.rows_affected(), 1);
    assert_eq!(assign_as_holder(&held, 0).await?, Vec::new());
    let after = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(after.run.live_run.is_none());
    assert!(after.run.earlier_waiting);
    held.fixtures.cleanup().await;
    Ok(())
}

/// A message far past the 8 KiB a person's steer was once held to folds into
/// the live run whole: 64 KiB admitted, the same 64 KiB assigned.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_message_over_8_kib_reaches_the_run() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let text = "line of a long brief\n".repeat(64 * 1024 / 21 + 1);
    assert!(text.len() > 64 * 1024, "{}", text.len());
    let body = serde_json::json!({ "message": text }).to_string();
    let sent = send(&held, &workspace, Producer::Steer, &body).await?;
    let live = sent.run.live_run.as_ref().ok_or("the held run is live")?;
    assert!(live.same_conversation, "the send folds: {:?}", sent.run);
    assert_eq!(assign_as_holder(&held, 0).await?, [sent.assigned(&text)]);
    held.fixtures.cleanup().await;
    Ok(())
}
