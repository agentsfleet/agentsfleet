//! Issue's refusals that are not about the assignment: a host that may not take
//! work, and an event the turn ledger never minted.
//!
//! `integration_run_issue.rs` proves the fenced insert. These prove the two
//! checks around it — the host row the insert locks, and the turn stamp
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
use afd_fleet::run::Billed;

use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// A host the daemon does not know, and a host whose verdict went degraded,
/// are issued no run; the assignment stays good, so the same host takes it once
/// its verdict clears.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_host_that_may_not_take_work_is_issued_no_run() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let runs = fixtures.runs();
    let now = fixtures.enrolled_at();
    let tenant_id = tenant.clone();
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let held = seed::select_fleet(&runs, &runner, &fleet)
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

    let unknown = runs
        .issue(&afd_db::test_util::mint_id(), &held, billed, now)
        .await
        .expect("an unknown host is a verdict");
    degrade(Some("cgroup controller missing")).await;
    let degraded = runs
        .issue(&runner, &held, billed, now)
        .await
        .expect("a degraded host is a verdict");
    degrade(None).await;
    let cleared = runs
        .issue(&runner, &held, billed, now)
        .await
        .expect("the healthy host's insert answers");

    assert!(unknown.is_none(), "a host with no row takes no work");
    assert!(degraded.is_none(), "a degraded host takes no work");
    assert!(cleared.is_some(), "the assignment survived both refusals");
    fixtures.cleanup().await;
}
