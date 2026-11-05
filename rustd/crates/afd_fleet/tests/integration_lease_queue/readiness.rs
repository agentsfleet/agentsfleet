//! The admission trigger keeps each fleet's readiness on its slot.

use std::time::Duration;

use afd_db::config::DbRole;
use afd_fleet::lease::admit::Refusal;

use super::{admit_at, created_at, fence, ready_at};
use crate::rows::Row;
use crate::seed::{claim_fleet, seeded_parts};
use crate::support::Fixtures;

/// The shipped slot, whose backfill the backfill case replays.
const READY_SLOT: i64 = 2190;
/// Where the slot's backfill begins.
const BACKFILL_MARK: &str = "-- Backfill:";

/// A refusal label the fixtures end an event with.
const ENDED: Refusal = Refusal::labelled("fixture_ended");

/// The first admission creates the slot at fence 0 and arms it
/// with its own instant; a second admission leaves the head's instant.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_admission_insert_sets_ready_at_to_the_head() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [_runner]) = seeded_parts::<1>(&fixtures).await;
    assert_eq!(fence(&fixtures, &fleet).await, None, "no slot before work");

    let head = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let head_at = created_at(&fixtures, &head).await;
    assert_eq!(
        fence(&fixtures, &fleet).await,
        Some(0),
        "a fresh slot at fence 0"
    );
    assert_eq!(ready_at(&fixtures, &fleet).await, head_at);

    let _second = admit_at(&fixtures, &fleet, &workspace, 1).await;
    assert_eq!(
        ready_at(&fixtures, &fleet).await,
        head_at,
        "a later admission leaves the head's place"
    );
    fixtures.cleanup().await;
}

/// Settling the head moves readiness to the next admission,
/// and settling the last clears it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_settling_the_head_moves_ready_at_to_the_next_admission() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let _first = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let second = admit_at(&fixtures, &fleet, &workspace, 1).await;
    let leases = fixtures.leases();

    let head = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("the head");
    leases.block(&head, ENDED).await.expect("the head ends");
    assert_eq!(
        ready_at(&fixtures, &fleet).await,
        created_at(&fixtures, &second).await,
        "the next admission is the head now"
    );

    let next = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("the next");
    assert_eq!(next.event_id, second);
    leases.block(&next, ENDED).await.expect("the last ends");
    assert_eq!(ready_at(&fixtures, &fleet).await, None, "nothing waits");
    fixtures.cleanup().await;
}

/// A lease whose runner stops renewing is claimed by another
/// runner once its deadline passes, with no sweep and no write between.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_lapsed_lease_is_claimable_without_any_write() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let event = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let leases = fixtures.leases();
    let held = claim_fleet(&leases, &holder, &fleet).await.expect("held");
    let armed = ready_at(&fixtures, &fleet).await;
    assert!(
        claim_fleet(&leases, &other, &fleet).await.is_none(),
        "a live lease is not claimable"
    );

    fixtures
        .age(
            Row::Slot(&fleet),
            "lease_expires_at",
            afd_core::timing::LEASE_TTL,
        )
        .await;
    let taken = claim_fleet(&leases, &other, &fleet)
        .await
        .expect("the lapsed lease is claimable");
    assert_eq!(
        (
            taken.event_id.as_str(),
            taken.fence > held.fence,
            ready_at(&fixtures, &fleet).await
        ),
        (event.as_str(), true, armed),
        "the same admission, under a higher fence, readiness untouched"
    );
    fixtures.cleanup().await;
}

/// The slot's shipped backfill arms exactly the fleets whose
/// head is unsettled and unparked.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_ready_backfill_matches_unsettled_heads() {
    let fixtures = Fixtures::create_isolated().await;
    let (ready, ready_ws, _, [_a]) = seeded_parts::<1>(&fixtures).await;
    let (parked, parked_ws, _, [_b]) = seeded_parts::<1>(&fixtures).await;
    let (idle, _, _, [_c]) = seeded_parts::<1>(&fixtures).await;
    let head = admit_at(&fixtures, &ready, &ready_ws, 0).await;
    let parked_head = admit_at(&fixtures, &parked, &parked_ws, 0).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    // Back to the state before the slot shipped: parked, no readiness, and
    // one fleet's slot never written.
    sqlx::query("UPDATE fleet.admissions SET parked_at = now() WHERE id = $1::uuid")
        .bind(&parked_head)
        .execute(&mut *connection)
        .await
        .expect("park");
    sqlx::raw_sql("ALTER TABLE fleet.admissions DISABLE TRIGGER trg_admissions_mark_ready")
        .execute(&mut *connection)
        .await
        .expect("the backfill runs without the trigger, as at migration");
    sqlx::query("UPDATE runner.fleet_slots SET ready_at = NULL WHERE fleet_id = ANY($1::uuid[])")
        .bind(vec![ready.clone(), parked.clone(), idle.clone()])
        .execute(&mut *connection)
        .await
        .expect("clear readiness");
    sqlx::query("DELETE FROM runner.fleet_slots WHERE fleet_id = $1::uuid")
        .bind(&ready)
        .execute(&mut *connection)
        .await
        .expect("a fleet with work and no slot");

    let backfill = afd_db::migration::MIGRATIONS
        .iter()
        .find(|migration| migration.version() == READY_SLOT)
        .expect("the readiness slot is registered")
        .sql()
        .split_once(BACKFILL_MARK)
        .and_then(|(_, tail)| tail.split_once('\n'))
        .map(|(_, statements)| statements)
        .expect("the slot carries its backfill");
    sqlx::raw_sql(sqlx::AssertSqlSafe(backfill.to_owned()))
        .execute(&mut *connection)
        .await
        .expect("the shipped backfill runs");
    sqlx::raw_sql("ALTER TABLE fleet.admissions ENABLE TRIGGER trg_admissions_mark_ready")
        .execute(&mut *connection)
        .await
        .expect("the trigger is back");
    drop(connection);
    owner.close().await;

    assert_eq!(
        (
            ready_at(&fixtures, &ready).await,
            fence(&fixtures, &ready).await,
            ready_at(&fixtures, &parked).await,
            ready_at(&fixtures, &idle).await,
        ),
        (created_at(&fixtures, &head).await, Some(0), None, None),
        "only the unparked head is armed, on a slot the backfill wrote"
    );
    fixtures.cleanup().await;
}

/// The trigger fires on a park too, through the
/// production park that frees the slot.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_park_disarms_the_fleet() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = admit_at(&fixtures, &fleet, &workspace, 0).await;
    super::approvals::pending_card(
        &fixtures,
        &fleet,
        &workspace,
        Some(&event),
        Duration::from_secs(3600),
    )
    .await;
    let leases = fixtures.leases();
    let held = claim_fleet(&leases, &runner, &fleet).await.expect("held");
    leases
        .park(&held.fleet_id, &held.event_id, held.fence)
        .await
        .expect("parked");
    assert_eq!(
        ready_at(&fixtures, &fleet).await,
        None,
        "a parked head is not ready"
    );
    assert!(
        fixtures.slot_is_free(&fleet).await,
        "the park frees the slot"
    );
    fixtures.cleanup().await;
}
