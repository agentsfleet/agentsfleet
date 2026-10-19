//! A fleet whose runner holds its sandbox, against live datastores: the report
//! records the hold under its fence, the heartbeat keeps it honest, and the
//! claim lets only the holder in while the hold is live.
//!
//! Each hold is made the way production makes one, through the claim and the
//! report's release, never by writing the row: a test that wrote `hold_expires_at`
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
mod beat;
mod claim;
mod counted;
mod lapse;
mod poll;
pub(crate) mod report;
mod resume;
mod unleasable;

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_core::timing::{HEARTBEAT_INTERVAL, LEASE_TTL, SANDBOX_HOLD_IDLE};
use afd_fleet::lease::Leases;
use afd_runner::Beat;
use afd_runner::heartbeat::holds::prove;
use afd_wire::runner::{HeartbeatRequest, HeldFleets};
use jiff::Timestamp;

use crate::requests::capable;
use crate::rows::Row;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// A fleet status other than active: what an owner's stop leaves.
const STOPPED: &str = "stopped";

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a seeded identifier")
}

/// The slot's hold and its last runner, as stored.
async fn slot(fixtures: &Fixtures, fleet: &str) -> (Option<Timestamp>, Option<String>) {
    (
        fixtures.instant(Row::Slot(fleet), "hold_expires_at").await,
        fixtures.text(Row::Slot(fleet), "last_runner_id").await,
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
    beats_at(fixtures, runner, &request, fixtures.enrolled_at()).await;
}

/// `runner` beats `request` at `now`, its holds list proved as the handler
/// proves it.
async fn beats_at(
    fixtures: &Fixtures,
    runner: &Uuid7,
    request: &HeartbeatRequest<'_>,
    now: Timestamp,
) -> Beat {
    let held = prove(request.holds.clone());
    fixtures
        .runners()
        .heartbeat(runner, request, held.as_ref(), now)
        .await
        .expect("a beat lands")
}

/// `runner` claims `fleet`, and its report releases the slot holding the
/// sandbox until `until`.
async fn hold(leases: &Leases, fixtures: &Fixtures, fleet: &str, runner: &Uuid7, until: Timestamp) {
    let claimed = leases
        .claim(&id(fleet), runner, LEASE_TTL)
        .await
        .expect("the claim answers")
        .expect("a free slot is won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(&mut connection, &id(fleet), claimed.fence, Some(until))
        .await
        .expect("the report's release lands");
}

/// Ages each of `fleets`' slots a full beat interval, as if their holds were
/// reported that long ago: a beat may then clear a hold its list leaves out.
async fn settle(fixtures: &Fixtures, fleets: &[&str]) {
    for fleet in fleets {
        fixtures
            .age(Row::Slot(fleet), "updated_at", HEARTBEAT_INTERVAL)
            .await;
    }
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
    let until = fixtures.after(SANDBOX_HOLD_IDLE);

    let first = leases
        .claim(&id(&fleet), &runner, LEASE_TTL)
        .await
        .expect("answers")
        .expect("won");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    leases
        .release_through(&mut connection, &id(&fleet), first.fence, Some(until))
        .await
        .expect("lands");
    let stored = slot(&fixtures, &fleet).await;
    let second = leases
        .claim(&id(&fleet), &runner, LEASE_TTL)
        .await
        .expect("answers")
        .expect("the runner wins its own fleet again");
    leases
        .release_through(&mut connection, &id(&fleet), first.fence, None)
        .await
        .expect("a stale release is a no-op, not an error");
    let after_stale = slot(&fixtures, &fleet).await;
    leases
        .release_through(&mut connection, &id(&fleet), second.fence, None)
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
    let until = fixtures.after(SANDBOX_HOLD_IDLE);
    live(&fixtures, &runner).await;
    hold(&leases, &fixtures, &kept, &runner, until).await;
    hold(&leases, &fixtures, &dropped, &runner, until).await;
    settle(&fixtures, &[&kept, &dropped]).await;
    let now = fixtures.after(HEARTBEAT_INTERVAL);

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
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(&fleet)
        .bind(STOPPED)
        .execute(&mut *connection)
        .await
        .expect("the owner stops the fleet");
    drop(connection);
    let now = fixtures.after(std::time::Duration::from_millis(1));

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
