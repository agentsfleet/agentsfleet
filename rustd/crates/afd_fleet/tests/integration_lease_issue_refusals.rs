//! Issue's refusals that are not about the claim: a host that may not take
//! work, and an event the admission ledger never minted.
//!
//! `integration_lease_issue.rs` proves the fenced insert. These prove the two
//! checks around it — the host row the insert locks, and the admission stamp
//! the narrative log owes — each answer a verdict rather than a fault.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::seed;
use crate::support;
use afd_core::id::Uuid7;
use afd_fleet::lease::{Billed, Issue};
use afd_observability::metrics::label::fleet::IssueDenied;

use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// A host the daemon does not know, and a host whose verdict went degraded,
/// are issued no lease; the claim stays good, so the same host takes it once
/// its verdict clears.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_host_that_may_not_take_work_is_issued_no_lease() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let now = fixtures.enrolled_at();
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let held = crate::seed::select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the fleet is leasable");
    let degrade = |reason: Option<&'static str>| {
        let fixtures = &fixtures;
        let runner = runner.as_str().to_owned();
        async move {
            sqlx::query("UPDATE runner.runners SET degraded_reason = $2 WHERE id = $1::uuid")
                .bind(runner)
                .bind(reason)
                .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
                .await
                .expect("the host's verdict is written");
        }
    };

    let unknown = leases
        .issue(&afd_db::test_util::mint_uuid7(), &held, billed, now)
        .await
        .expect("an unknown host is a verdict");
    degrade(Some("cgroup controller missing")).await;
    let degraded = leases
        .issue(&runner, &held, billed, now)
        .await
        .expect("a degraded host is a verdict");
    degrade(None).await;
    let cleared = leases
        .issue(&runner, &held, billed, now)
        .await
        .expect("the healthy host's insert answers");

    assert_eq!(
        unknown,
        Issue::Denied(IssueDenied::HostMissing),
        "a host with no row takes no work"
    );
    assert_eq!(
        degraded,
        Issue::Denied(IssueDenied::Capacity),
        "a degraded host takes no work"
    );
    assert!(
        matches!(cleared, Issue::Issued(_)),
        "the claim survived both refusals"
    );
    fixtures.cleanup().await;
}

/// A claim without its durable queue row cannot create narrative history.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_event_absent_from_the_ledger_cannot_open_history() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases_with_dead_queue();
    let minted = crate::seed::select_fleet(&leases, &runner, &fleet)
        .await
        .expect("claim");
    // History opens only through the claim, and only for the admission it
    // took: one row, and it is that admission's.
    let opened: Vec<String> =
        sqlx::query_scalar("SELECT event_id FROM fleet.events WHERE fleet_id=$1::uuid")
            .bind(&fleet)
            .fetch_all(&mut *fixtures.database.acquire().await.expect("database"))
            .await
            .expect("history rows");
    assert_eq!(opened, vec![minted.event_id.clone()]);
    fixtures.cleanup().await;
}
