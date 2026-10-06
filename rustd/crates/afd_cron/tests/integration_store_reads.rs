//! The schedule table's reads: a list, one schedule, and a fire's target.
//!
//! # Every assertion is scoped to this lane's own fleet
//!
//! Postgres is shared across the lane, so a count or a list over the whole
//! table would race whatever else is running. Each fixture mints its own
//! workspace and fleet, and every read below is filtered by them. Split from
//! `integration_store.rs`, which keeps the create and its refusals.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::DesiredStatus;

use self::support::CronLane;

/// An expression every fixture registers under.
const NIGHTLY: &str = "0 3 * * *";

// ── Reads are scoped by fleet, always ────────────────────────────────────────

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_list_carries_this_fleets_schedules_and_no_others() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    lane.create("key-a", NIGHTLY).await;
    lane.create("key-b", "0 4 * * *").await;
    other.create("key-c", NIGHTLY).await;

    let listed = lane
        .store
        .list(&lane.fleet_id())
        .await
        .expect("the lane answers");

    assert_eq!(
        listed.len(),
        2,
        "another fleet's schedule reached this list"
    );
    assert!(
        listed.iter().all(|it| it.fleet_id == lane.fleet_id()),
        "every row a fleet is shown must be its own"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_schedule_is_not_readable_through_a_fleet_that_does_not_own_it() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let created = lane.create("key-scoped", NIGHTLY).await;

    assert!(
        lane.store
            .one(&other.fleet_id(), &created.schedule_id)
            .await
            .expect("the lane answers")
            .is_none(),
        "the fleet in the path is a filter, not decoration"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_list_for_a_fleet_with_no_schedules_is_empty_rather_than_absent() {
    let lane = CronLane::open().await;

    let listed = lane
        .store
        .list(&lane.fleet_id())
        .await
        .expect("the lane answers");

    assert!(
        listed.is_empty(),
        "a fleet that has registered nothing has an empty list, not a failure"
    );
}

// ── What a signed fire resolves to ───────────────────────────────────────────

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_fire_resolves_to_the_fleet_workspace_and_message_it_wakes() {
    let lane = CronLane::open().await;
    let created = lane.create("key-fire", NIGHTLY).await;

    let target = lane
        .store
        .fire_target(&created.schedule_id)
        .await
        .expect("the lane answers")
        .expect("a live schedule resolves");

    assert_eq!(target.fleet, lane.fleet_id());
    assert_eq!(target.workspace, lane.workspace_id());
    assert_eq!(target.message, "run the nightly repair");
    assert_eq!(target.desired_status, DesiredStatus::Active);
    assert_eq!(
        target.fleet_status, "active",
        "the fleet's own status travels with the target, so the fire path can \
         drop a delivery for a paused fleet without a second query"
    );
}

/// A callback for a schedule that is gone resolves to nothing, not to an error.
///
/// The external scheduler keeps its own copy, so a delete leaves fires already
/// scheduled. The sender is correctly configured and acting on what it was last
/// told, so this is dropped rather than refused.
#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_fire_for_a_schedule_this_daemon_no_longer_has_resolves_to_nothing() {
    let lane = CronLane::open().await;
    let created = lane.settled("key-vanished", NIGHTLY).await;
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
    assert!(
        lane.store
            .delete_claimed(&held, &token)
            .await
            .expect("the lane answers")
    );

    assert!(
        lane.store
            .fire_target(&created.schedule_id)
            .await
            .expect("the lane answers")
            .is_none()
    );
}

/// A stored word this build cannot read fails the read rather than vanishing.
///
/// Three of this table's columns hold a closed vocabulary — `source`,
/// `desired_status`, `sync_status`. A row carrying anything else is one written
/// by a build that knew a word this one does not: an older or newer daemon, or
/// an operator editing by hand.
///
/// The read has to FAIL there rather than skip the row. A list that quietly
/// dropped what it could not parse would answer "this fleet has no schedules"
/// for a fleet with one firing every night, and the operator would go on to
/// create a second. Failing loud makes that a visible incident instead.
///
/// Planted with a statement rather than through the store, because the store's
/// own writer cannot produce this state — every word it writes is one this
/// build parses, which is exactly the property being relied on.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_row_carrying_a_word_this_build_cannot_read_fails_the_read() {
    let lane = CronLane::open().await;
    let readable = lane.create("decode-readable", NIGHTLY).await;

    // One statement per column rather than a built string: sqlx requires a
    // `&'static str`, and that restriction is a SQL-injection guard worth
    // keeping even where the value is a literal in a test.
    for (column, statement, unreadable) in [
        (
            "source",
            "UPDATE core.fleet_schedules SET source = $2 WHERE id = $1::uuid",
            "carrier-pigeon",
        ),
        (
            "desired_status",
            "UPDATE core.fleet_schedules SET desired_status = $2 WHERE id = $1::uuid",
            "hibernating",
        ),
        (
            "sync_status",
            "UPDATE core.fleet_schedules SET sync_status = $2 WHERE id = $1::uuid",
            "pending",
        ),
    ] {
        {
            let mut connection = lane.connection().await;
            sqlx::query(statement)
                .bind(readable.schedule_id.as_str())
                .bind(unreadable)
                .execute(&mut *connection)
                .await
                .expect("planting an unreadable word")
        };

        assert!(
            lane.store.list(&lane.fleet_id()).await.is_err(),
            "`{column} = {unreadable}` must fail the read — a list that dropped \
             the row it could not parse answers \"no schedules\" for a fleet \
             that has one firing every night"
        );

        {
            let mut connection = lane.connection().await;
            sqlx::query(
                "UPDATE core.fleet_schedules \
                 SET source = 'api', desired_status = 'active', sync_status = 'syncing' \
                 WHERE id = $1::uuid",
            )
            .bind(readable.schedule_id.as_str())
            .execute(&mut *connection)
            .await
            .expect("restoring the row between cases")
        };
    }

    // Each case restored its row, so the lane reads normally again: the
    // failures above belong to the planted word, not to a lane left broken.
    assert!(
        lane.store.list(&lane.fleet_id()).await.is_ok(),
        "a table holding only words this build knows reads normally"
    );
}
