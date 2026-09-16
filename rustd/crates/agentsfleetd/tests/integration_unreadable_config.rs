//! One fleet's unreadable configuration refuses that fleet, not the deployment.
//!
//! # The hazard, which this lane produced on its own
//!
//! A runner does not poll one fleet. Readiness is sixteen partitions and a
//! rotation visits every one, so a runner's polls reach every fleet the
//! deployment holds. The pull path resolves a fleet's stored configuration —
//! it is what the money gates price and the approval gate judges — and that
//! read used to leave the path as an error, which the router answered as a
//! 500. So one fleet carrying a document this daemon cannot parse refused
//! every runner, for every fleet, once per rotation: six of this suite's own
//! tests failed that way against a shared database, each reporting
//! `UZ-INTERNAL-003 fleet configuration unreadable` from a poll that had
//! nothing to do with the broken fleet.
//!
//! The decision recorded against it is that one bad document is one fleet's
//! problem. This is the test that holds the daemon to it.
//!
//! # Why the neighbour's config is `{}` exactly
//!
//! Not an arbitrary broken value: it is what the `afd_fleet` store suites
//! leave in a shared lane database. They call `Leases::select` and `issue`
//! directly, and only the PULL path resolves a configuration, so a fixture
//! that never crosses this seam has never needed a document a fleet could
//! really carry. Reproducing their row is what makes this test the lane's own
//! failure rather than an invented one.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::event::EventType;
use agentsfleetd::supervisor::Supervisor;

use crate::e2e::{Scenario, scenario};
use crate::e2e_event::enqueue;
use crate::reads::event_column_of;
use crate::wire::{capable_beat, poll_for_seeded_lease, poll_until, post, report_body};

/// The document the store suites leave behind: valid JSON, and not a fleet.
const UNREADABLE_CONFIG: &str = "{}";

/// The label the refusal writes, as an operator reads it.
const CONFIG_UNREADABLE: &str = "config_unreadable";

/// The terminal status a refused event carries.
const GATE_BLOCKED: &str = "gate_blocked";

/// A second fleet in the scenario's workspace, holding a config nothing parses.
///
/// Its identifier is the scenario's own with a distinct trailing slot, so it
/// satisfies the column's version-7 CHECK and cannot collide with any other
/// scenario's fleet in a shared lane.
async fn seed_unreadable_fleet(run: &Scenario) -> String {
    let fleet = format!("{}00f0", &run.fleet[..run.fleet.len() - 4]);
    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8)",
    )
    .bind(&fleet)
    .bind(&run.workspace)
    .bind(&run.tenant)
    .bind("e2e-unreadable-fleet")
    .bind("# fixture")
    .bind(UNREADABLE_CONFIG)
    .bind("active")
    .bind(run.seeded_at.as_millis())
    .execute(&mut *run.booted.database.acquire().await.expect("connection"))
    .await
    .expect("the neighbour fleet must insert");
    fleet
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_an_unreadable_config_refuses_its_own_fleet_only() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200, "the runner proves itself");

    let poisoned = seed_unreadable_fleet(&run).await;
    let poisoned_event = enqueue(&run.booted, &poisoned, &run.workspace, EventType::Chat).await;

    // The deployment still serves work. Every poll on the way is asserted to
    // answer 200, which is the regression this file exists for: before the
    // fix, the poll that sampled the neighbour's partition answered 500 and
    // this loop died there rather than finding the scenario's own event.
    let (lease_id, fence) = poll_for_seeded_lease(&http, &run).await;

    // Settled so the runner's one lease slot is free for the polls below.
    let settled = post(
        &http,
        &run,
        "/v1/runners/me/reports",
        &report_body(&lease_id, &run.event_id, fence),
    )
    .await;
    assert_eq!(settled.status().as_u16(), 200, "the report is accepted");

    // And the neighbour is ended rather than retried forever, which is what
    // puts it in front of a human.
    let refused = poll_until(&http, &run, || async {
        event_column_of(&run, &poisoned, &poisoned_event, "status").await
            == Some(GATE_BLOCKED.to_owned())
    })
    .await;
    assert!(refused, "the unreadable fleet's own event is ended");
    assert_eq!(
        event_column_of(&run, &poisoned, &poisoned_event, "failure_label").await,
        Some(CONFIG_UNREADABLE.to_owned()),
        "and the row names the document rather than the runner or the money"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
