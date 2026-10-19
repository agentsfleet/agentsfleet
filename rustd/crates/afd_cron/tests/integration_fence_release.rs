//! Giving the single-syncer fence back: a finalize records whether the push
//! worked, a delete removes the row once upstream agrees, and both land only
//! for the syncer that still holds the claim.
//!
//! Split from `integration_fence.rs`, which takes the fence. `Ok(None)` here,
//! as there, means "not mine, or not there", and every case reads the row
//! afterwards to say which.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::SyncStatus;

use self::support::CronLane;

/// The finalize is conditioned on the claim, not merely on the row existing.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_finalize_from_a_syncer_that_lost_the_row_does_not_land() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-lost", "0 3 * * *").await;
    let fleet = lane.fleet_id();

    let loser = CronLane::token();
    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &loser)
        .await
        .expect("the lane answers")
        .expect("the first claim takes an unheld row");

    // The lease runs out and a successor takes the row, exactly as it would if
    // the first syncer's process had stopped mid-push.
    lane.expire_lease(&created.schedule_id).await;
    let winner = CronLane::token();
    lane.store
        .claim_current(&fleet, &created.schedule_id, &winner)
        .await
        .expect("the lane answers")
        .expect("the expired row is claimable");

    // The first syncer now comes back and reports success against the state it
    // was holding.
    let landed = lane
        .store
        .finalize_synced(&held, &loser, None)
        .await
        .expect("the lane answers");

    assert!(
        landed.is_none(),
        "the newer holder's state is the right one; overwriting it is precisely \
         what the fence exists to prevent"
    );

    let current = lane
        .store
        .one(&fleet, &created.schedule_id)
        .await
        .expect("the lane answers")
        .expect("the row is still there");
    assert_eq!(
        current.sync_status,
        SyncStatus::Syncing,
        "the row must still belong to the winner, mid-push"
    );
    assert_eq!(current.sync_token.as_deref(), Some(winner.as_str()));
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_finalize_from_the_holder_releases_the_fence_and_records_success() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-synced", "0 3 * * *").await;
    let fleet = lane.fleet_id();
    let token = CronLane::token();

    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &token)
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let released = lane
        .store
        .finalize_synced(&held, &token, None)
        .await
        .expect("the lane answers")
        .expect("the holder's finalize lands");

    assert_eq!(released.sync_status, SyncStatus::Synced);
    assert!(
        released.sync_token.is_none(),
        "a released fence must not still name a holder, or the next claim waits \
         out a lease nobody is using"
    );
    assert!(
        released.last_error.is_none(),
        "a success must clear the reason a previous failure left"
    );
}

/// A failure is a STORED state, which is the whole reason `/sync` can retry it.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_failed_push_keeps_its_reason_and_stays_retryable() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-failed", "0 3 * * *").await;
    let fleet = lane.fleet_id();
    let token = CronLane::token();

    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &token)
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let reason = "the external scheduler refused the request with status 429";
    let released = lane
        .store
        .finalize_failed(&held, &token, reason)
        .await
        .expect("the lane answers")
        .expect("the holder's finalize lands");

    assert_eq!(released.sync_status, SyncStatus::Failed);
    assert_eq!(
        released.last_error.as_deref(),
        Some(reason),
        "the vendor's own answer belongs here, where an operator reads it — it \
         is deliberately not what the caller was told"
    );
    assert!(
        released.sync_token.is_none(),
        "a failure releases the fence too, or the retry would wait out a lease"
    );

    // Retryable means exactly this: the next claim takes it.
    let retry = CronLane::token();
    assert!(
        lane.store
            .claim_current(&fleet, &created.schedule_id, &retry)
            .await
            .expect("the lane answers")
            .is_some(),
        "`/sync` must be able to pick a failed schedule back up"
    );
}

/// A delete is a finalize that removes the row, and it is fenced identically.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_delete_from_a_syncer_that_lost_the_row_removes_nothing() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-delete-lost", "0 3 * * *").await;
    let fleet = lane.fleet_id();
    let before = lane.count().await;

    let loser = CronLane::token();
    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &loser)
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    lane.expire_lease(&created.schedule_id).await;
    let winner = CronLane::token();
    lane.store
        .claim_current(&fleet, &created.schedule_id, &winner)
        .await
        .expect("the lane answers")
        .expect("the expired row is claimable");

    let removed = lane
        .store
        .delete_claimed(&held, &loser)
        .await
        .expect("the lane answers");

    assert!(
        !removed,
        "a stale claim must not delete a row it no longer holds"
    );
    assert_eq!(
        lane.count().await,
        before,
        "the row is still this fleet's, and still there"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_delete_from_the_holder_removes_the_row() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-delete", "0 3 * * *").await;
    let fleet = lane.fleet_id();
    let before = lane.count().await;
    let token = CronLane::token();

    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &token)
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    assert!(
        lane.store
            .delete_claimed(&held, &token)
            .await
            .expect("the lane answers"),
        "the holder's delete lands"
    );
    assert_eq!(lane.count().await, before - 1);
    assert!(
        lane.store
            .one(&fleet, &created.schedule_id)
            .await
            .expect("the lane answers")
            .is_none(),
        "the row is gone, not merely unreadable"
    );
}
