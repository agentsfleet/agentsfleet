//! The single-syncer fence: who may push a schedule upstream, and who may say
//! it worked.
//!
//! # What the fence is actually preventing
//!
//! Two daemons behind one load balancer both see a schedule that needs
//! pushing. Without a fence both push it, both write back, and the row ends up
//! describing whichever finished last — which may be the one that pushed the
//! OLDER intent. The token plus the version make that impossible to express:
//! a claim takes the token and bumps the version, and every finalize is
//! conditioned on still holding both.
//!
//! `Ok(None)` throughout means "not mine, or not there". It is an answer and
//! not a failure, and every case below asserts which of the two it means by
//! reading the row afterwards. Releasing the fence lives in
//! `integration_fence_release.rs`.
//!
//! # Why the lease is moved rather than waited out
//!
//! `SYNC_LEASE` is thirty seconds. A suite that waited it out would be the
//! slowest in the lane and would prove only that the clock advances. Moving the
//! stored deadline into the past is the same state an abandoned syncer leaves
//! behind, reached in one statement.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use std::time::Duration;

use afd_core::clock;
use afd_cron::{Change, DesiredStatus, SYNC_LEASE, SyncStatus};

use self::support::CronLane;

/// How long a claim statement may take between stamping `updated_at` with
/// `now()` and reading `clock_timestamp()` for the lease.
const STATEMENT_SLACK: Duration = Duration::from_secs(5);

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
async fn a_claim_takes_the_token_and_moves_the_version() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-claim", "0 3 * * *").await;
    let token = CronLane::token();

    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &token)
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    assert_eq!(held.sync_token.as_deref(), Some(token.as_str()));
    assert_eq!(held.sync_status, SyncStatus::Syncing);
    assert!(
        held.resource_version > created.resource_version,
        "the version must move so a finalize from the previous holder cannot land: \
         {} then {}",
        created.resource_version,
        held.resource_version
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
        .claim_current(&fleet, &created.schedule_id, &first)
        .await
        .expect("the lane answers");
    assert!(held.is_some(), "the first claim takes an unheld row");

    let contender = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &second)
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
        .claim_current(&fleet, &created.schedule_id, &abandoned)
        .await
        .expect("the lane answers")
        .expect("the first claim takes an unheld row");
    lane.expire_lease(&created.schedule_id).await;

    let successor = CronLane::token();
    let taken = lane
        .store
        .claim_current(&fleet, &created.schedule_id, &successor)
        .await
        .expect("the lane answers")
        .expect("an expired lease is not a hold");

    assert_eq!(
        taken.sync_token.as_deref(),
        Some(successor.as_str()),
        "a schedule stuck behind a dead syncer would never sync again"
    );
}

/// A lease is set on the database's clock: the deadline is the claim's own
/// stamp plus `SYNC_LEASE`, give or take the statement's run time, so a
/// syncer's wall clock cannot widen or shorten it.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_lease_is_set_on_the_database_clock() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-lease-clock", "0 3 * * *").await;
    let held = lane
        .store
        .claim_current(&lane.fleet_id(), &created.schedule_id, &CronLane::token())
        .await
        .expect("the lane answers")
        .expect("an unheld schedule is claimable");

    let deadline = held.sync_lease_expires_at.expect("a claim sets a lease");
    let earliest = clock::after(held.updated_at, SYNC_LEASE);
    assert!(
        deadline >= earliest && deadline <= clock::after(earliest, STATEMENT_SLACK),
        "{deadline} is the claim's stamp {} plus the lease",
        held.updated_at
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
        .claim_current(&fleet, &created.schedule_id, &CronLane::token())
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
        .claim_current(&other.fleet_id(), &created.schedule_id, &CronLane::token())
        .await
        .expect("the lane answers");

    assert!(
        claimed.is_none(),
        "the fleet in the path is a filter, not decoration: without it a caller \
         proven on one fleet could push another fleet's schedule"
    );
}
