//! Racing claims on one ready fleet: one wins, and every loser writes nothing
//! and waits on nothing.
//!
//! The claim takes the slot and its head admission with `SKIP LOCKED`, so a
//! claim that meets a slot another claim holds moves past it rather than
//! queueing behind it, and every write in the statement joins the pick, so a
//! claim that took nothing changed no row.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER};
use afd_fleet::lease::sql::queue::CLAIM_HEAD;
use afd_fleet::lease::sql::{
    FLEET_STATUS_ACTIVE, LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, RUNNER_STATUS_ACTIVE,
};
use sqlx::Connection as _;

use crate::seed::{Seeded, seeded};
use crate::support::Fixtures;

/// How many polls race for the one fleet.
const POLLS: usize = 100;

/// How long the poll may take before it counts as waiting on the open claim.
const ANSWERED_WITHIN: Duration = Duration::from_secs(5);

/// Sessions queued behind `$1`.
const WAITERS: &str = "SELECT count(*) FROM pg_stat_activity \
     WHERE $1::integer = ANY(pg_blocking_pids(pid))";

/// The slot's fence, deadline and readiness, as text so a moved instant shows.
const SLOT: &str = "SELECT fencing_token, lease_expires_at::text, ready_at::text \
     FROM runner.fleet_slots WHERE fleet_id = $1::uuid";

/// Which runner the slot names now.
const HOLDER: &str =
    "SELECT last_runner_id::text FROM runner.fleet_slots WHERE fleet_id = $1::uuid";

/// The fleet's oldest unsettled admission, locked as an open writer would.
const LOCK_HEAD: &str = "SELECT id FROM fleet.admissions \
     WHERE fleet_id = $1::uuid AND settled_at IS NULL ORDER BY sequence LIMIT 1 FOR UPDATE";

type Slot = (i64, String, Option<String>);

async fn slot(fixtures: &Fixtures, fleet: &str) -> Slot {
    sqlx::query_as(SLOT)
        .bind(fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the slot reads back")
}

/// Dimension 2.2: a hundred polls released together against one ready fleet.
/// One claims it, ninety-nine answer nothing, and the fence moved once.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_hundred_pollers_claim_one_fleet_once() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners, fleet, ..
    } = seeded::<4>(&fixtures).await;
    let before = slot(&fixtures, &fleet).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(POLLS));
    let polls: Vec<_> = (0..POLLS)
        .map(|index| {
            let leases = fixtures.leases();
            let runner = runners
                .get(index % runners.len())
                .expect("a runner per slot in the rotation")
                .clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                leases.select(&runner, &[]).await
            })
        })
        .collect();
    let mut claimed = Vec::new();
    let mut empty = 0_usize;
    for poll in polls {
        match poll
            .await
            .expect("the poll joins")
            .expect("a lost race is not a fault")
        {
            Some(acquired) => claimed.push(acquired),
            None => empty += 1,
        }
    }
    let after = slot(&fixtures, &fleet).await;

    assert_eq!(
        (claimed.len(), empty),
        (1, POLLS - 1),
        "one claim wins and every other answers nothing"
    );
    assert_eq!(
        after.0,
        before.0 + 1,
        "the fence moved once: no loser bumped it"
    );
    assert_eq!(
        claimed.first().map(|acquired| acquired.fleet_id.as_str().to_owned()),
        Some(fleet.clone())
    );
    fixtures.cleanup().await;
}

/// Dimension 2.3: a ready slot whose head admission an open transaction holds
/// is skipped, and the claim that skipped it changed nothing on the slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_empty_claim_writes_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let before = slot(&fixtures, &fleet).await;
    let mut other = fixtures.database.acquire().await.expect("a connection");
    let mut holding = other.begin().await.expect("a transaction");
    sqlx::query(LOCK_HEAD)
        .bind(&fleet)
        .fetch_one(&mut *holding)
        .await
        .expect("the head is held");

    let polled = tokio::time::timeout(ANSWERED_WITHIN, fixtures.leases().select(&runner, &[]))
        .await
        .expect("the claim never waits on the held head")
        .expect("a skipped head is not a fault");
    let after = slot(&fixtures, &fleet).await;
    holding.rollback().await.expect("the head is let go");

    assert!(polled.is_none(), "the held head is skipped: {polled:?}");
    assert_eq!(
        after, before,
        "fence, deadline and readiness are untouched by the empty claim"
    );
    fixtures.cleanup().await;
}

/// A claim that meets a slot another, uncommitted claim holds answers nothing
/// at once: it queues behind nobody, and the slot stays the first claimer's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_claim_that_meets_a_live_claim_takes_nothing_and_never_waits() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let fleet_id = Uuid7::parse(&fleet).expect("a seeded fleet id");
    let mut other = fixtures.database.acquire().await.expect("a connection");
    let mut claiming = other.begin().await.expect("a transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *claiming)
        .await
        .expect("the session names itself");
    sqlx::query(CLAIM_HEAD)
        .bind(second.as_str())
        .bind(Vec::<String>::new())
        .bind(LEASE_TTL)
        .bind(RUNNER_OFFLINE_AFTER)
        .bind(RUNNER_STATUS_ACTIVE)
        .bind(LEASE_STATUS_ACTIVE)
        .bind(LEASE_STATUS_EXPIRED)
        .bind(FLEET_STATUS_ACTIVE)
        .bind(afd_core::event::status::RECEIVED)
        .bind(afd_admission::Producer::Steer.as_str())
        .bind(None::<&str>)
        .fetch_optional(&mut *claiming)
        .await
        .expect("the other runner's claim wins, uncommitted")
        .expect("the other runner takes the fleet");

    let polled = tokio::time::timeout(
        ANSWERED_WITHIN,
        fixtures.leases().select(&first, &[fleet_id]),
    )
    .await
    .expect("the claim never waits on the open one")
    .expect("a lost claim is not a fault");
    let queued: i64 = sqlx::query_scalar(WAITERS)
        .bind(pid)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the waiters count");
    claiming
        .commit()
        .await
        .expect("the other runner's claim commits");
    drop(other);

    assert_eq!(queued, 0, "nothing queued behind the open claim");
    assert!(polled.is_none(), "the held slot is skipped: {polled:?}");
    let holder: String = sqlx::query_scalar(HOLDER)
        .bind(&fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the slot reads back");
    assert_eq!(holder, second.as_str(), "the slot is the first claimer's");
    fixtures.cleanup().await;
}
