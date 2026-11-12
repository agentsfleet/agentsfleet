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
use afd_fleet::lease::Billed;

use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// A host the daemon does not know, and a host whose verdict went degraded,
/// are issued no lease; the claim stays good, so the same host takes it once
/// its verdict clears.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_host_that_may_not_take_work_is_issued_no_lease() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let now = fixtures.enrolled_at();
    let tenant_id = tenant.clone();
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let held = seed::select_fleet(&leases, &runner, &fleet)
        .await
        .expect("the fleet is leasable");
    let degrade = |reason: Option<&'static str>| {
        let fixtures = &fixtures;
        let runner = runner.clone();
        async move {
            sqlx::query("UPDATE runner.runners SET degraded_reason = $2 WHERE id = $1")
                .bind(runner)
                .bind(reason)
                .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
                .await
                .expect("the host's verdict is written");
        }
    };

    let unknown = leases
        .issue(&afd_db::test_util::mint_id(), &held, billed, now)
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

    assert!(unknown.is_none(), "a host with no row takes no work");
    assert!(degraded.is_none(), "a degraded host takes no work");
    assert!(cleared.is_some(), "the claim survived both refusals");
    fixtures.cleanup().await;
}
