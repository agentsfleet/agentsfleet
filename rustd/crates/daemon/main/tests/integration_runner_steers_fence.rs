//! M252 §2 — a runner that lost its lease puts no work on another fleet.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a step indexes the JSON it was answered"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::steers::Crew;

/// Dimension 2.5. A token below the fleet's live sequence, and a holder a
/// reclaim superseded, are refused with no row written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_refuses_a_stale_fencing_token() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (lease, fence) = crew.speaking.lease_seeded().await;

    let (status, refused) = crew.steer((&lease, fence - 1), &target, "late").await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-RUN-005"), &json!("superseded")),
        "{refused}"
    );
    crew.speaking.supersede().await;
    let (status, refused) = crew.steer((&lease, fence), &target, "late").await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-RUN-005"), &json!("superseded")),
        "{refused}"
    );
    let rows = crew.admissions(&target).await;
    assert!(rows.is_empty(), "{rows:?}");
    crew.finish(supervisor).await;
}
