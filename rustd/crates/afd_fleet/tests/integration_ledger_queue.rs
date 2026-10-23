//! Ledger claims use real runtime-role transactions and no working Dragonfly.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a failed fixture precondition fails the test"
)]

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Leases};
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

use crate::{seed::seeded_parts, support::Fixtures};

mod contention;
mod deadlines;
mod delivery;
mod ordering;
mod parking;
mod parking_commit;
mod reclaim;
mod settlement;

const WITHIN: Duration = Duration::from_secs(10);
const INSERT: &str = "INSERT INTO fleet.admissions \
    (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, \
     request_json, event_created_at, created_at, updated_at) \
    VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $1, 'fixture-digest', 'fixture:steer', \
            $5, '{\"prompt\":\"fixture\"}', $6, $6, $6)";
const PARK: &str = "UPDATE fleet.admissions SET parked_at = CASE WHEN $2 THEN now() END \
    WHERE id = $1::uuid";
const FINISH: &str = "UPDATE fleet.admissions SET settled_at = now() WHERE id = $1::uuid";
const STAMPS: &str = "SELECT a.lease_fence, s.fencing_token FROM fleet.admissions a \
    LEFT JOIN runner.fleet_slots s ON s.fleet_id = a.fleet_id WHERE a.id = $1::uuid";
const LOCK: &str = "SELECT id::text FROM fleet.admissions WHERE id = $1::uuid FOR UPDATE";

async fn enqueue(fixtures: &Fixtures, fleet: &str, workspace: &str) -> String {
    let id = afd_db::test_util::mint_id();
    sqlx::query(INSERT)
        .bind(&id)
        .bind(fleet)
        .bind(workspace)
        .bind(afd_admission::Producer::Steer.as_str())
        .bind(afd_wire::event::EventType::Chat.as_str())
        .bind(fixtures.enrolled_at().to_sqlx())
        .execute(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("runtime connection"),
        )
        .await
        .expect("accepted fixture row");
    id
}

async fn stamps(fixtures: &Fixtures, event: &str) -> (Option<i64>, Option<i64>) {
    sqlx::query_as(STAMPS)
        .bind(event)
        .fetch_one(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("runtime connection"),
        )
        .await
        .expect("queue and slot fences")
}

async fn next(leases: &Leases, runner: &Uuid7) -> Option<Acquired> {
    tokio::time::timeout(WITHIN, leases.select(runner, &[]))
        .await
        .expect("claim never waits on the admission head")
        .expect("valid claim")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_fleet_rows_lease_in_order_behind_a_park() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases_with_dead_queue();
    let mut events = Vec::new();
    for _ in 0..3 {
        events.push(enqueue(&fixtures, &fleet, &workspace).await);
    }
    let head = events.first().expect("three admissions");
    parking::seed_gate(&fixtures, &fleet, &workspace).await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("runtime connection");
    sqlx::query(PARK)
        .bind(head)
        .bind(true)
        .execute(&mut *connection)
        .await
        .expect("park head");
    assert!(
        next(&leases, &runner).await.is_none(),
        "park holds later rows"
    );
    assert_eq!(stamps(&fixtures, head).await, (None, None));
    sqlx::query(PARK)
        .bind(head)
        .bind(false)
        .execute(&mut *connection)
        .await
        .expect("answer gate");
    for event in events {
        let acquired = next(&leases, &runner).await.expect("unparked head");
        assert_eq!(acquired.event_id, event);
        finish(&leases, &mut connection, acquired).await;
    }
    assert!(
        next(&leases, &runner).await.is_none(),
        "settled rows stay out"
    );
    drop(connection);
    drop(leases);
    fixtures.cleanup().await;
}

/// Advance the ordering fixture; the benchmark separately drives real reports.
async fn finish(leases: &Leases, connection: &mut sqlx::PgConnection, acquired: Acquired) {
    sqlx::query(FINISH)
        .bind(&acquired.event_id)
        .execute(connection)
        .await
        .expect("fixture settled");
    leases
        .release(&acquired.fleet_id, acquired.fence)
        .await
        .expect("slot free");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_locked_head_does_not_expose_the_next_row_or_keep_a_slot() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let head = enqueue(&fixtures, &fleet, &workspace).await;
    let _later = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("runtime connection");
    let mut transaction = connection.begin().await.expect("head lock transaction");
    sqlx::query(LOCK)
        .bind(&head)
        .fetch_one(&mut *transaction)
        .await
        .expect("head locked");
    assert!(next(&leases, &runner).await.is_none());
    assert_eq!(
        stamps(&fixtures, &head).await,
        (None, None),
        "empty claim rolls the slot back"
    );
    transaction.rollback().await.expect("unlock head");
    assert_eq!(
        next(&leases, &runner)
            .await
            .expect("head available")
            .event_id,
        head
    );
    drop(connection);
    drop(leases);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_decode_fault_rolls_back_both_the_fence_and_the_admission() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    // Postgres accepts this timestamp, while the application timestamp bridge
    // refuses it. Failure occurs after slot and admission updates in the query.
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("runtime connection");
    sqlx::query("UPDATE fleet.admissions SET event_created_at = 'infinity' WHERE id = $1::uuid")
        .bind(&event)
        .execute(&mut *connection)
        .await
        .expect("inject decode failure");
    let leases = fixtures.leases_with_dead_queue();
    let refused = leases
        .select(&runner, &[])
        .await
        .expect_err("invalid timestamp refuses claim");
    assert!(
        refused.to_string().contains("admissions ledger claim"),
        "{refused}"
    );
    assert_eq!(stamps(&fixtures, &event).await, (None, None));
    sqlx::query("UPDATE fleet.admissions SET event_created_at = $2 WHERE id = $1::uuid")
        .bind(&event)
        .bind(fixtures.enrolled_at().to_sqlx())
        .execute(&mut *connection)
        .await
        .expect("remove injected fault");
    let acquired = next(&leases, &runner)
        .await
        .expect("no leaked lock or slot");
    assert_eq!(acquired.fence.as_i64(), 1);
    assert_eq!(stamps(&fixtures, &event).await, (Some(1), Some(1)));
    drop(connection);
    drop(leases);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_locked_slot_is_skipped_without_changing_the_admission() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &runner).await.expect("first claim");
    let mut connection = fixtures.database.acquire().await.expect("database");
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at = now()-interval '1 second' WHERE fleet_id=$1::uuid")
        .bind(&fleet).execute(&mut *connection).await.expect("expire slot");
    let mut lock = connection.begin().await.expect("lock transaction");
    sqlx::query("SELECT fleet_id FROM runner.fleet_slots WHERE fleet_id=$1::uuid FOR UPDATE")
        .bind(&fleet)
        .execute(&mut *lock)
        .await
        .expect("hold slot");
    assert!(next(&leases, &runner).await.is_none());
    assert_eq!(
        stamps(&fixtures, &event).await.0,
        Some(first.fence.as_i64())
    );
    lock.rollback().await.expect("release slot");
    assert!(
        next(&leases, &runner)
            .await
            .expect("claim after release")
            .fence
            > first.fence
    );
    drop(connection);
    fixtures.cleanup().await;
}
