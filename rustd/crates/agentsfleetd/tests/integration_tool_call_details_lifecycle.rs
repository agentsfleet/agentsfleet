//! A tool call's records over their lifetime: they go with their event, and
//! the runtime role may do everything the verb and the read do to them.
//!
//! Split from `integration_tool_call_details.rs` at the length cap.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]

use agentsfleetd::supervisor::Supervisor;

use crate::e2e::scenario;
use crate::integration_tool_call_details::{execute, leased, post_records, record, rows, scalar};

/// Dimension 1.7. Records go with their event.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_event_delete_cascades_tool_call_details() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[record(1, "x")]).await;
    assert_eq!(status, 200);
    execute(
        &run,
        "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .await;
    assert_eq!(rows(&run).await, 0);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.8. The runtime role holds every privilege the verb and the
/// read use.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_details_grants_allow_runtime() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    for privilege in ["INSERT", "UPDATE", "SELECT", "DELETE"] {
        let granted = scalar(
            &run,
            &format!(
                "SELECT (has_table_privilege('api_runtime', \
                 'core.fleet_tool_call_details', '{privilege}') \
                 AND $1 <> '' AND $2 <> '')::int::bigint"
            ),
            None,
        )
        .await;
        assert_eq!(granted, 1, "api_runtime lacks {privilege}");
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}
