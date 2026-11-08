//! Which messages a claim answers, in which order, and to whom.

use afd_admission::{Producer, Reply};

use super::support::{
    Checked, STEER_JSON, STEER_TEXT, WEBHOOK_JSON, claim, claim_as_holder, park, send, send_as,
    stamp_claim, workspace_of,
};
use crate::report_seed::held;

/// A person's later steers in the run's conversation fold, oldest first, as
/// the text the model would read had they started the run; the send reads as
/// one that folds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_live_run_claims_a_persons_messages_oldest_first() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let first = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let second = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let live = second
        .run
        .live_run
        .as_ref()
        .ok_or("the held lease is live")?;
    assert!(
        !live.durable_task && live.same_conversation && !second.run.earlier_waiting,
        "the send folds: {:?}",
        second.run
    );
    assert_eq!(
        claim_as_holder(&held, 0).await?,
        [first.claimed(STEER_TEXT), second.claimed(STEER_TEXT)]
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
    let machine = afd_events::ACTOR_MACHINE;
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
            claim_as_holder(&held, 0).await?,
            [before.claimed(STEER_TEXT)],
            "{producer:?} by {actor} (seq {}) is neither folded nor passed",
            barrier.sequence
        );
        held.fixtures.cleanup().await;
    }
    Ok(())
}

/// A parked message is never passed: nothing after it is claimed, and a
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
        claim_as_holder(&held, 0).await?,
        [before.claimed(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A claim never replaces a stamp a later fence wrote: the message stays
/// with the lease that claimed it last.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_never_replaces_a_later_fences_stamp() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let later = i64::try_from(held.fence.as_u64())? + 1;
    stamp_claim(&held, &sent.id, later).await?;
    assert_eq!(claim_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// Only the runner holding the lease, under its current fence, claims.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runner_or_a_superseded_fence_claims_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let fence = held.fence.as_u64();
    assert_eq!(claim(&held, &held.spare, fence, 0).await?, Vec::new());
    assert_eq!(
        claim(&held, &held.runner, fence + 1, 0).await?,
        Vec::new(),
        "a fence the slot never issued claims nothing"
    );
    assert_eq!(
        claim(&held, &held.runner, fence, 0).await?,
        [sent.claimed(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A claim whose answer was lost reads the same messages again, and the
/// cursor moves past what the run read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_repeated_claim_answers_the_same_messages() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let first = claim_as_holder(&held, 0).await?;
    assert_eq!(first, [sent.claimed(STEER_TEXT)]);
    assert_eq!(claim_as_holder(&held, 0).await?, first);
    assert_eq!(claim_as_holder(&held, sent.sequence).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// Internal setup keys are not admission identifiers, so they cannot own folds.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_internal_setup_event_is_not_an_admission_head() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let before = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(before.run.live_run.is_some());
    let changed = sqlx::query(
        "UPDATE fleet.events SET event_id=$2 WHERE id=(SELECT fleet_event_id FROM runner.leases WHERE id=$1::uuid)",
    )
    .bind(held.lease_key.as_str())
    .bind("setup_ready:1")
    .execute(&mut *held.fixtures.database.acquire().await?)
    .await?;
    assert_eq!(changed.rows_affected(), 1);
    assert_eq!(claim_as_holder(&held, 0).await?, Vec::new());
    let after = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert!(after.run.live_run.is_none());
    assert!(after.run.earlier_waiting);
    held.fixtures.cleanup().await;
    Ok(())
}
