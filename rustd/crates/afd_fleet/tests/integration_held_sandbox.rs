//! A fleet whose runner holds its sandbox, against live datastores: the report
//! records the hold under its fence, the heartbeat keeps it honest, and the
//! claim lets only the holder in while the hold is live.
//!
//! Each hold is made the way production makes one, through the claim and the
//! report's release, never by writing the row: a test that wrote `held_until`
//! by hand would prove the predicates against a row no runner could produce.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles these without
//! datastores; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

// Child of this suite, not a crate-root module: it reaches its parent's
// helpers through `super::`, so it must stay nested here. The path is
// relative to THIS file's directory, which the aggregator does not change.
#[path = "integration_held_sandbox/beat.rs"]
mod beat;
#[path = "integration_held_sandbox/claim.rs"]
mod claim;
#[path = "integration_held_sandbox/counted.rs"]
mod counted;
#[path = "integration_held_sandbox/lapse.rs"]
mod lapse;
#[path = "integration_held_sandbox/poll.rs"]
mod poll;
#[path = "integration_held_sandbox/report.rs"]
mod report;
#[path = "integration_held_sandbox/resume.rs"]
mod resume;
#[path = "integration_held_sandbox/unleasable.rs"]
mod unleasable;

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::{HEARTBEAT_INTERVAL_MS, LEASE_TTL_MS, SANDBOX_HOLD_IDLE_MS};
use afd_fleet::lease::Leases;
use afd_runner::Beat;
use afd_runner::heartbeat::holds::prove;
use afd_wire::runner::{HeartbeatRequest, HeldFleets};
use sqlx::Row as _;

use crate::requests::{ENROLLED_AT, capable};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// A fleet status other than active: what an owner's stop leaves.
const STOPPED: &str = "stopped";

fn at(millis: i64) -> UnixMillis {
    UnixMillis::from_millis(millis)
}

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a seeded identifier")
}

/// The slot's hold and its last runner, as stored.
async fn slot(fixtures: &Fixtures, fleet: &str) -> (Option<i64>, Option<String>) {
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    let row = sqlx::query(
        "SELECT held_until, last_runner_id::text FROM fleet.runner_affinity \
         WHERE fleet_id = $1::uuid",
    )
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("the fleet has a slot");
    (
        row.try_get(0).expect("held_until"),
        row.try_get(1).expect("last_runner_id"),
    )
}

/// `runner` beats once as a host proving its whole assignment, holding
/// nothing yet, which is what makes its holds bind: a runner enrolled and
/// never heard from is silent, and one that reported nothing reads degraded
/// and leases nothing.
async fn live(fixtures: &Fixtures, runner: &Uuid7) {
    let request = HeartbeatRequest {
        capability_report: Some(capable()),
        ..beat(&[])
    };
    beats_at(fixtures, runner, &request, ENROLLED_AT).await;
}

/// `runner` beats `request` at `now`, its holds list proved as the handler
/// proves it.
async fn beats_at(
    fixtures: &Fixtures,
    runner: &Uuid7,
    request: &HeartbeatRequest<'_>,
    now: i64,
) -> Beat {
    let held = prove(request.holds.clone());
    fixtures
        .runners()
        .heartbeat(runner, request, held.as_ref(), at(now))
        .await
        .expect("a beat lands")
}

/// `runner` claims `fleet`, and its report releases the slot holding the
/// sandbox until `until`.
async fn hold(leases: &Leases, fixtures: &Fixtures, fleet: &str, runner: &Uuid7, until: i64) {
    let now = at(ENROLLED_AT);
    let claimed = leases
        .claim(&id(fleet), runner, now, LEASE_TTL_MS)
        .await
        .expect("the claim answers")
        .expect("a free slot is won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(
            &mut connection,
            &id(fleet),
            claimed.fence,
            Some(at(until)),
            now,
        )
        .await
        .expect("the report's release lands");
}

fn beat<'a>(holds: &'a [&'a str]) -> HeartbeatRequest<'a> {
    HeartbeatRequest {
        capability_report: None,
        selftest: None,
        holds: HeldFleets(holds.iter().map(|fleet| Cow::Borrowed(*fleet)).collect()),
        closing: false,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_stores_hold_under_fencing() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let now = at(ENROLLED_AT);
    let until = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;

    let first = leases
        .claim(&id(&fleet), &runner, now, LEASE_TTL_MS)
        .await
        .expect("answers")
        .expect("won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(
            &mut connection,
            &id(&fleet),
            first.fence,
            Some(at(until)),
            now,
        )
        .await
        .expect("lands");
    let stored = slot(&fixtures, &fleet).await;
    let later = at(ENROLLED_AT + 1);
    let second = leases
        .claim(&id(&fleet), &runner, later, LEASE_TTL_MS)
        .await
        .expect("answers")
        .expect("the runner wins its own fleet again");
    leases
        .release_through(&mut connection, &id(&fleet), first.fence, None, later)
        .await
        .expect("a stale release is a no-op, not an error");
    let after_stale = slot(&fixtures, &fleet).await;
    leases
        .release_through(&mut connection, &id(&fleet), second.fence, None, later)
        .await
        .expect("lands");
    drop(connection);

    assert_eq!(stored, (Some(until), Some(runner.as_str().to_owned())));
    assert_eq!(after_stale.0, Some(until), "a stale fence records nothing");
    assert_eq!(
        slot(&fixtures, &fleet).await.0,
        None,
        "the live fence clears it"
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_heartbeat_clears_dropped_holds() {
    let fixtures = Fixtures::create_with_queue().await;
    let (kept, _workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let (dropped, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let until = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;
    live(&fixtures, &runner).await;
    hold(&leases, &fixtures, &kept, &runner, until).await;
    hold(&leases, &fixtures, &dropped, &runner, until).await;
    // A full interval after the holds were reported, so the beat may clear.
    let now = ENROLLED_AT + HEARTBEAT_INTERVAL_MS;

    let both = beats_at(&fixtures, &runner, &beat(&[&kept, &dropped]), now).await;
    let one = beats_at(&fixtures, &runner, &beat(&[&kept]), now).await;

    assert!(both.release_holds.is_empty() && one.release_holds.is_empty());
    assert_eq!(slot(&fixtures, &kept).await.0, Some(until));
    assert_eq!(slot(&fixtures, &dropped).await.0, None);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_inactive_fleet_hold_destroyed() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [runner, other]) = seeded_parts::<2>(&fixtures).await;
    let (foreign, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &runner).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &runner,
        ENROLLED_AT + SANDBOX_HOLD_IDLE_MS,
    )
    .await;
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query("UPDATE core.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(&fleet)
        .bind(STOPPED)
        .execute(&mut *connection)
        .await
        .expect("the owner stops the fleet");
    drop(connection);
    let now = ENROLLED_AT + 1;

    let stopped = beats_at(&fixtures, &runner, &beat(&[&fleet]), now).await;
    let stranger = beats_at(&fixtures, &other, &beat(&[&foreign]), now).await;

    assert_eq!(stopped.release_holds, [fleet]);
    assert_eq!(
        stranger.release_holds,
        [foreign],
        "a fleet this runner never held is answered as one to drop"
    );
    fixtures.cleanup().await;
}
