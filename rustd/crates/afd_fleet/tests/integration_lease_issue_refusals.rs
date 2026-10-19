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

use crate::queue;
use crate::seed;
use crate::support;
use afd_core::id::Uuid7;
use afd_fleet::lease::{Billed, Delivery};

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
    let held = crate::seed::select_fleet_within_rotations(&leases, &runner, &fleet)
        .await
        .expect("the fleet is leasable");
    leases
        .record_received(&held)
        .await
        .expect("the narrative log must open");
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

    assert!(unknown.is_none(), "a host with no row takes no work");
    assert!(degraded.is_none(), "a degraded host takes no work");
    assert!(cleared.is_some(), "the claim survived both refusals");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// An event whose id the admission ledger never minted — an approval's
/// continuation, or one that predates the ledger — opens its log with no
/// admission to stamp, and its redelivery is told apart as a repeat.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_event_the_ledger_never_minted_opens_its_log_unstamped() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let minted = crate::seed::select_fleet_within_rotations(&leases, &runner, &fleet)
        .await
        .expect("the fleet is leasable");
    let continuation = afd_fleet::lease::Acquired {
        event_id: format!("continuation-{}", minted.event_id),
        ..minted
    };

    let first = leases
        .record_received(&continuation)
        .await
        .expect("the narrative log opens");
    let again = leases
        .record_received(&continuation)
        .await
        .expect("the redelivery answers");

    assert_eq!(first.delivery, Delivery::First);
    assert_eq!(again.delivery, Delivery::Repeat);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM fleet.events WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(&fleet)
    .bind(&continuation.event_id)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the opened row reads back");
    assert_eq!(status, afd_core::event::status::RECEIVED);
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
