//! The single-syncer fence: who may push a schedule upstream, and who may say
//! it worked.
//!
//! # What the fence is actually preventing
//!
//! Two daemons behind one load balancer both see a schedule that needs
//! pushing. Without a fence both push it, both write back, and the row ends up
//! describing whichever finished last — which may be the one that pushed the
//! OLDER intent. The token plus the generation make that impossible to express:
//! a claim takes the token and bumps the generation, and every finalize is
//! conditioned on still holding both.
//!
//! `Ok(None)` throughout means "not mine, or not there". It is an answer and
//! not a failure, and every case below asserts which of the two it means by
//! reading the row afterwards.
//!
//! # Why the lease is moved rather than waited out
//!
//! `SYNC_LEASE_MS` is thirty seconds. A suite that waited it out would be the
//! slowest in the lane and would prove only that the clock advances. Moving the
//! stored deadline into the past is the same state an abandoned syncer leaves
//! behind, reached in one statement.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::{Change, DesiredStatus, SyncStatus};

use self::support::CronLane;

/// A change that alters nothing, for the cases about the FENCE rather than the
/// edit it carries.
const NO_EDIT: Change<'static> = Change {
    cron: None,
    timezone: None,
    message: None,
    desired_status: None,
};

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_claim_takes_the_token_and_moves_the_generation() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-claim", "0 3 * * *").await;
    let token = CronLane::token();

    let held = lane
        .store
        .claim_current(
            &lane.fleet_id(),
            &created.schedule_id,
            &token,
            CronLane::now(),
        )
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    assert_eq!(held.sync_token.as_deref(), Some(token.as_str()));
    assert_eq!(held.sync_status, SyncStatus::Syncing);
    assert!(
        held.generation > created.generation,
        "the generation must move so a finalize from the previous holder cannot land: \
         {} then {}",
        created.generation,
        held.generation
    );
}

/// The property the whole table exists for.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_second_syncer_cannot_take_a_schedule_the_first_still_holds() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-contended", "0 3 * * *").await;
    let fleet = lane.fleet_id();
    let first = CronLane::token();
    let second = CronLane::token();

    let held = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &first, CronLane::now())
        .await
        .expect("the lane answers");
    assert!(held.is_some(), "the first claim takes an unheld row");

    let contender = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &second, CronLane::now())
        .await
        .expect("the lane answers");

    assert!(
        contender.is_none(),
        "two syncers holding one schedule is exactly what the fence prevents"
    );
}

/// A syncer that died must not hold the row forever.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_lease_that_ran_out_releases_the_row_to_the_next_syncer() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-expired", "0 3 * * *").await;
    let fleet = lane.fleet_id();

    let abandoned = CronLane::token();
    lane.store
        .claim_current(&fleet, &created.schedule_id, &abandoned, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("the first claim takes an unheld row");
    lane.expire_lease(&created.schedule_id).await;

    let successor = CronLane::token();
    let taken = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &successor, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("an expired lease is not a hold");

    assert_eq!(
        taken.sync_token.as_deref(),
        Some(successor.as_str()),
        "a schedule stuck behind a dead syncer would never sync again"
    );
}

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
        .claim_current(&fleet, &created.schedule_id, &loser, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("the first claim takes an unheld row");

    // The lease runs out and a successor takes the row, exactly as it would if
    // the first syncer's process had stopped mid-push.
    lane.expire_lease(&created.schedule_id).await;
    let winner = CronLane::token();
    lane.store
        .claim_current(&fleet, &created.schedule_id, &winner, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("the expired row is claimable");

    // The first syncer now comes back and reports success against the state it
    // was holding.
    let landed = lane
        .store
        .finalize_synced(&held, &loser, None, CronLane::now())
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
        .claim_current(&fleet, &created.schedule_id, &token, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let released = lane
        .store
        .finalize_synced(&held, &token, None, CronLane::now())
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
        .claim_current(&fleet, &created.schedule_id, &token, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let reason = "the external scheduler refused the request with status 429";
    let released = lane
        .store
        .finalize_failed(&held, &token, reason, CronLane::now())
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
            .claim_current(&fleet, &created.schedule_id, &retry, CronLane::now())
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
        .claim_current(&fleet, &created.schedule_id, &loser, CronLane::now())
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    lane.expire_lease(&created.schedule_id).await;
    let winner = CronLane::token();
    lane.store
        .claim_current(&fleet, &created.schedule_id, &winner, CronLane::now())
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
        .claim_current(&fleet, &created.schedule_id, &token, CronLane::now())
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

/// A claim carrying an edit applies it and takes the fence in ONE statement.
///
/// Two statements would leave a window where the row holds the new intent and
/// no token, and a syncer arriving in that window would push an intent nobody
/// had claimed responsibility for.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_claim_carrying_an_edit_applies_it_and_takes_the_fence_together() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-edit", "0 3 * * *").await;
    let token = CronLane::token();

    let held = lane
        .store
        .claim_change(
            &lane.fleet_id(),
            &created.schedule_id,
            Change {
                cron: Some("30 4 * * *"),
                desired_status: Some(DesiredStatus::Paused),
                ..NO_EDIT
            },
            &token,
            CronLane::now(),
        )
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    assert_eq!(held.cron, "30 4 * * *");
    assert_eq!(held.desired_status, DesiredStatus::Paused);
    assert_eq!(held.sync_token.as_deref(), Some(token.as_str()));
    assert_eq!(
        held.sync_status,
        SyncStatus::Syncing,
        "the edit is the operator's intent; upstream has not been told yet"
    );
}

/// An edit cannot jump the queue while another syncer holds the row.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn an_edit_is_refused_while_another_syncer_holds_the_row() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-edit-held", "0 3 * * *").await;
    let fleet = lane.fleet_id();

    lane.store
        .claim_current(
            &fleet,
            &created.schedule_id,
            &CronLane::token(),
            CronLane::now(),
        )
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let editor = lane
        .store
        .claim_change(
            &fleet,
            &created.schedule_id,
            Change {
                cron: Some("0 5 * * *"),
                ..NO_EDIT
            },
            &CronLane::token(),
            CronLane::now(),
        )
        .await
        .expect("the lane answers");

    assert!(editor.is_none(), "the fence covers edits as well as pushes");

    let current = lane
        .store
        .one(&fleet, &created.schedule_id)
        .await
        .expect("the lane answers")
        .expect("the row is there");
    assert_eq!(
        current.cron, "0 3 * * *",
        "a refused edit must not have half-applied"
    );
}

/// A schedule is addressed by fleet AND id, so another fleet cannot reach it.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_schedule_cannot_be_claimed_through_a_fleet_that_does_not_own_it() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let created = lane.settled("key-foreign", "0 3 * * *").await;

    let claimed = lane
        .store
        .claim_current(
            &other.fleet_id(),
            &created.schedule_id,
            &CronLane::token(),
            CronLane::now(),
        )
        .await
        .expect("the lane answers");

    assert!(
        claimed.is_none(),
        "the fleet in the path is a filter, not decoration: without it a caller \
         proven on one fleet could push another fleet's schedule"
    );
}
