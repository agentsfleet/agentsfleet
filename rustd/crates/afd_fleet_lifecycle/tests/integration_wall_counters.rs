//! What the wall reads beside the set, and what a page shows for a fleet that
//! has never run.
//!
//! Both proven against live Postgres, because the claim in each is about a
//! `COALESCE` over a correlated subselect: a stub would prove the read CALLS
//! something, not that a fleet with no counter row answers with zeros rather
//! than dropping out. `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::tail::FleetCounters;

use afd_fleet_lifecycle::{Install, LibrarySource};

use crate::integration_patch_visibility::installed;
use crate::support::{LIBRARY_ID, Lane, mint};

/// The page size the reads below ask for; larger than any lane seeds.
const PAGE: u32 = 50;

/// Where a fleet that has run stands, as its trigger would have left it.
const RAN: FleetCounters = FleetCounters {
    events_processed: 3,
    budget_used_nanos: 21,
};

/// Seeds the counter row the triggers maintain, for a fleet that "ran".
async fn seed_counters(lane: &Lane, fleet: &Uuid7, counters: FleetCounters) {
    sqlx::query(
        "INSERT INTO core.fleet_activity_counters
           (fleet_id, events_processed, budget_used_nanos, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $4, $4)",
    )
    .bind(fleet.as_str())
    .bind(counters.events_processed)
    .bind(counters.budget_used_nanos)
    .bind(Lane::now().as_millis())
    .execute(&mut *lane.pool.acquire().await.expect("a pooled connection"))
    .await
    .expect("the counter row must insert");
}

/// Dimension 2.2. A fleet with no counter row still lists, with zeros: the
/// page reads the counters by key under `COALESCE`, where an inner join would
/// drop the fleet from the wall the moment it was installed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_fleet_with_no_counter_row_still_lists() {
    let lane = Lane::create().await;
    let ran = installed(&lane).await;
    let idle = installed(&lane).await;
    seed_counters(&lane, &ran.id, RAN).await;

    let page = lane
        .fleets
        .page(&lane.workspace, None, PAGE)
        .await
        .expect("the workspace pages");
    let row = |fleet: &Uuid7| {
        page.rows
            .iter()
            .find(|row| row.id == fleet.as_str())
            .expect("an installed fleet lists whether or not it has run")
    };
    let idle_row = row(&idle.id);
    assert_eq!(
        idle_row.events_processed, 0,
        "a fleet that never ran lists with zeros"
    );
    assert_eq!(idle_row.budget_used_nanos, 0);
    let ran_row = row(&ran.id);
    assert_eq!(ran_row.events_processed, RAN.events_processed);
    assert_eq!(ran_row.budget_used_nanos, RAN.budget_used_nanos);

    lane.cleanup().await;
}

/// The `hello`'s read answers every fleet in the set — zeros for one that has
/// never run — and nothing for an identifier that names no fleet, so a client
/// assigns every tile from the map and a stray id cannot invent a tile.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn the_wall_counters_answer_every_fleet_in_the_set() {
    let lane = Lane::create().await;
    let ran = installed(&lane).await;
    let idle = installed(&lane).await;
    seed_counters(&lane, &ran.id, RAN).await;
    let stranger = mint();

    let set = [
        ran.id.as_str().to_owned(),
        idle.id.as_str().to_owned(),
        stranger.as_str().to_owned(),
    ];
    let counters = lane
        .fleets
        .counters(&lane.workspace, &set)
        .await
        .expect("the set reads");

    assert_eq!(counters.get(ran.id.as_str()), Some(&RAN));
    assert_eq!(
        counters.get(idle.id.as_str()),
        Some(&FleetCounters {
            events_processed: 0,
            budget_used_nanos: 0,
        }),
        "a fleet that never ran answers, with zeros"
    );
    assert_eq!(
        counters.get(stranger.as_str()),
        None,
        "an identifier naming no fleet is not invented"
    );
    assert_eq!(counters.len(), 2);

    let empty = lane
        .fleets
        .counters(&lane.workspace, &[])
        .await
        .expect("an empty set reads");
    assert!(
        empty.is_empty(),
        "an empty wall asks for nothing and gets nothing"
    );

    lane.cleanup().await;
}

/// A read the datastore refuses is reported as the datastore's, so the wall's
/// `hello` can tell an outage from an empty set and send the set without its
/// figures rather than with zeros. No lane: the pool opens no socket.
#[tokio::test]
async fn a_refused_counters_read_is_reported_as_the_datastores() {
    let error = Lane::with_dead_database()
        .counters(&mint(), &[mint().as_str().to_owned()])
        .await
        .expect_err("nothing listens on the fixture port");
    assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
}

/// The statement is workspace-scoped like every sibling: a fleet another
/// workspace holds answers no row, so a caller that hands over a stray id
/// learns nothing about somebody else's spend.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_neighbours_fleet_is_not_answered_for() {
    let lane = Lane::create().await;
    let own = installed(&lane).await;
    let neighbour = lane.another_workspace().await;
    let theirs = lane
        .fleets
        .install(
            &neighbour,
            &Install {
                source: LibrarySource::Platform(LIBRARY_ID),
                name: None,
            },
            Lane::now(),
        )
        .await
        .expect("the neighbour's fleet installs");
    seed_counters(&lane, &theirs.id, RAN).await;

    let counters = lane
        .fleets
        .counters(
            &lane.workspace,
            &[own.id.as_str().to_owned(), theirs.id.as_str().to_owned()],
        )
        .await
        .expect("the set reads");
    assert!(counters.contains_key(own.id.as_str()));
    assert_eq!(
        counters.get(theirs.id.as_str()),
        None,
        "another workspace's fleet is absent, not zeroed and not disclosed"
    );

    lane.cleanup().await;
}
