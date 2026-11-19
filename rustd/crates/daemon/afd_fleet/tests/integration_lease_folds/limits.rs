//! What stops a claim: a lease that is over, a durable task, the per-claim
//! cap, and a message that is already settled.

use af_wire::fold::FOLDS_PER_CLAIM_MAX;
use afd_turns::Producer;

use super::support::{
    Checked, Claimed, STEER_JSON, STEER_TEXT, claim_as_holder, send, set_lease_task,
    settle_directly, workspace_of,
};
use crate::lease_reads::COLUMN_LEASED_UNTIL;
use crate::report_seed::held;
use crate::rows::{LAPSE, Row};

/// A lease past its own deadline claims nothing, though its slot still names
/// it: a holder that stopped renewing must not keep reading.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_lease_past_its_deadline_claims_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(claim_as_holder(&held, 0).await?, [sent.claimed(STEER_TEXT)]);
    let lease = Row::Lease(&held.issued.lease_id);
    held.fixtures.age(lease, COLUMN_LEASED_UNTIL, LAPSE).await;
    assert_eq!(claim_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// A slot past its deadline claims nothing, though its lease row is still
/// live: the slot is what a later holder displaces.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_slot_past_its_deadline_claims_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(claim_as_holder(&held, 0).await?, [sent.claimed(STEER_TEXT)]);
    let slot = Row::Slot(&held.fleet);
    held.fixtures.age(slot, COLUMN_LEASED_UNTIL, LAPSE).await;
    assert_eq!(claim_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// A durable task's lease claims nothing here: its conversation is claimed by
/// the task's own input path. The same lease claims again once it is not one.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_durable_task_lease_claims_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    set_lease_task(&held, Some(&afd_db::test_util::mint_id())).await?;
    assert_eq!(claim_as_holder(&held, 0).await?, Vec::new());
    set_lease_task(&held, None).await?;
    assert_eq!(claim_as_holder(&held, 0).await?, [sent.claimed(STEER_TEXT)]);
    held.fixtures.cleanup().await;
    Ok(())
}

/// A burst is read in batches: one more than the cap claims the cap, oldest
/// first, and the next claim from where the run read to takes the rest.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_takes_the_cap_and_the_next_takes_the_rest() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let cap = usize::from(FOLDS_PER_CLAIM_MAX);
    let mut sent = Vec::new();
    for _ in 0..=cap {
        sent.push(send(&held, &workspace, Producer::Steer, STEER_JSON).await?);
    }
    let first = claim_as_holder(&held, 0).await?;
    let oldest: Vec<Claimed> = sent
        .iter()
        .take(cap)
        .map(|message| message.claimed(STEER_TEXT))
        .collect();
    assert_eq!(first, oldest);
    let read_through = first
        .last()
        .ok_or("the first claim took messages")?
        .sequence;
    let newest = sent.last().ok_or("a burst was sent")?;
    assert_eq!(
        claim_as_holder(&held, read_through).await?,
        [newest.claimed(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A message an earlier run already settled is not claimed again; the one
/// behind it is.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_settled_message_is_not_claimed_again() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let done = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let waiting = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    settle_directly(&held, &done.id).await?;
    assert_eq!(
        claim_as_holder(&held, 0).await?,
        [waiting.claimed(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// Privacy cleanup can remove the task reference while retaining its generation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retained_task_generation_stops_the_ordinary_fold_prefix() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let earlier = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let barrier = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let later = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query("UPDATE fleet.admissions SET task_generation=1 WHERE id=$1 AND task_id IS NULL")
        .bind(&barrier.id)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert_eq!(
        claim_as_holder(&held, 0).await?,
        [earlier.claimed(STEER_TEXT)]
    );
    assert_eq!(claim_as_holder(&held, earlier.sequence).await?, Vec::new());
    let mut connection = held.fixtures.database.acquire().await?;
    let untouched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE id=ANY($1::uuid[])
        AND input_claim_lease_id IS NULL AND input_claim_fence IS NULL AND settled_at IS NULL",
    )
    .bind([barrier.id, later.id].as_slice())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(untouched, 2);
    Ok(())
}
