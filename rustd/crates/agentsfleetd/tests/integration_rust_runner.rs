//! The Rust runner's wire, against the daemon that ships.
//!
//! The runner reads the daemon's replies leniently, so a daemon that grows a
//! field never strands a runner built before it. The other direction stays
//! closed: what a runner WRITES is refused when it names a key the daemon does
//! not carry, because a runner believing something about the protocol that is
//! not true must find out at the first request, not in production.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::paths::RUNNER_REPORTS;
use agentsfleetd::supervisor::Supervisor;

use crate::e2e::scenario;
use crate::tail::lease;
use crate::wire::{post, report_body};

/// A key no build of the daemon carries.
const FUTURE: &str = "future";

/// A report naming an unknown key is refused, and the lease survives to take
/// the correct one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_runner_body_with_unknown_field_refused() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let mut body = report_body(&lease_id, &run.event_id, fence);
    let report = body.as_object_mut().expect("a report is a JSON object");
    report.insert(FUTURE.to_owned(), serde_json::Value::from(1));

    let refused = post(&http, &run, RUNNER_REPORTS, &body).await;
    assert_eq!(refused.status().as_u16(), 400, "an unknown key is refused");

    body.as_object_mut()
        .expect("a report is a JSON object")
        .remove(FUTURE);
    let accepted = post(&http, &run, RUNNER_REPORTS, &body).await;
    assert_eq!(
        accepted.status().as_u16(),
        200,
        "the refusal settled nothing, so the corrected report lands"
    );

    supervisor.shutdown().await;
}
