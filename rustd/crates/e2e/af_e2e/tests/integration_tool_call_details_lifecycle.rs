//! A tool call's records over their lifetime: they go with their event, and
//! the runtime role may do everything the verb and the read do to them.
//!
//! Split from `integration_tool_call_details.rs` at the length cap.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.

use agentsfleetd::supervisor::Supervisor;

use crate::e2e::{Scenario, scenario};
use crate::integration_tool_call_details::{
    execute, held, post_records, record, rows, rows_at, scalar,
};
use crate::wire::report_body;

/// Dimension 1.7. Records go with their event.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_event_delete_cascades_tool_call_details() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[record(1, "x")]).await;
    assert_eq!(status, 200);
    execute(
        &scene,
        "DELETE FROM thread.turns WHERE thread_id = $1 AND id = $2",
    )
    .await;
    assert_eq!(rows(&scene).await, 0);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Dimension 1.8. The runtime role holds every privilege the verb and the
/// read use.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_details_grants_allow_runtime() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    for privilege in ["INSERT", "UPDATE", "SELECT", "DELETE"] {
        let granted = scalar(
            &scene,
            &format!(
                "SELECT (has_table_privilege('api_runtime', \
                 'thread.items', '{privilege}') \
                 AND $1 IS NOT NULL AND $2 IS NOT NULL)::int::bigint"
            ),
            None,
        )
        .await;
        assert_eq!(granted, 1, "api_runtime lacks {privilege}");
    }
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A dead run of the scenario's turn under `fence`, and its record, as a
/// reassignment would have left them: the run a copy of the live one, one
/// fence below it.
async fn plant_dead_record(scene: &Scenario, fence: u64) {
    let (dead, record) = (afd_db::test_util::mint_id(), afd_db::test_util::mint_id());
    execute(
        scene,
        &format!(
            "WITH dead AS ( \
               INSERT INTO thread.runs \
                 (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at, \
                  posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
                  max_runtime_seconds, status, posted_message_count, created_at, updated_at) \
               SELECT '{dead}'::uuid, runner_id, fleet_id, workspace_id, tenant_id, turn_id, \
                      turn_created_at, posture, provider, model, last_metered_at, {fence}, \
                      to_timestamp(0), max_runtime_seconds, 'expired', 0, now(), now() \
               FROM thread.runs WHERE fleet_id = $1 AND turn_id = $2 \
                 AND fencing_token = {fence} + 1 \
               RETURNING id, workspace_id, fleet_id, turn_id) \
             INSERT INTO thread.items \
             (id, workspace_id, fleet_id, run_id, turn_id, fencing_token, sequence, kind, tool, \
              status, arguments, truncated_arguments, output, output_line_count, truncated, \
              byte_count, created_at, updated_at) \
             SELECT '{record}'::uuid, workspace_id, fleet_id, id, turn_id, {fence}, 1, \
                    'tool_call', 'shell', 'succeeded', '{{}}'::jsonb, false, 'dead', 1, false, \
                    256, now(), now() \
             FROM dead"
        ),
    )
    .await;
}

/// Dimension 1.6. Settlement keeps only the settling fence's records, and the
/// current run's first post already clears a dead run's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_settle_drops_dead_run_details() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    plant_dead_record(&scene, fence - 1).await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[record(1, "live")]).await;
    assert_eq!(status, 200);
    assert_eq!(
        (
            rows_at(&scene, fence - 1).await,
            rows_at(&scene, fence).await
        ),
        (0, 1),
        "the current run's post clears the dead run's record"
    );

    plant_dead_record(&scene, fence - 1).await;
    let settled = crate::wire::report(&scene, &report_body(&run_id, &scene.turn_id, fence)).await;
    assert!(settled.is_ok(), "native report refused: {settled:?}");
    assert_eq!(
        rows_at(&scene, fence - 1).await,
        0,
        "settlement drops it too"
    );
    assert_eq!(rows_at(&scene, fence).await, 1, "the settling run's stays");
    supervisor.shutdown().await;
    scene.cleanup().await;
}
