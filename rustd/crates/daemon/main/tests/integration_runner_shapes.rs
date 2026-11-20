//! The daemon populates runtime, usage and memory rows through a full runner cycle.
#![cfg(feature = "test-util")]
#![allow(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{MODEL, POSTURE, Scenario, scenario};
use crate::reads::{assert_shape, line_output_tokens, relay_settled, run_column};
use crate::wire::{
    MEMORY_CATEGORY, MEMORY_CONTENT, MEMORY_KEY, OUTPUT_TOKENS, capable_beat, post, report_body,
    wait_for_seeded_assignment,
};

/// rows, so the report can rebuild the price without re-resolving the tenant.
///
/// `posted_message_count` is `NOT NULL`, so every run carries it from the
/// moment it is written, and the messages verb counts against it under the
/// run's fence. The run names its event by its id (`turn_id`); the
/// actor and type are read from the event, not copied onto the run.
const RUN_SHAPE: &[&str] = &[
    "id",
    "runner_id",
    "fleet_id",
    "workspace_id",
    "tenant_id",
    "turn_id",
    "turn_created_at",
    "posture",
    "provider",
    "model",
    "last_metered_at",
    "fencing_token",
    "run_expires_at",
    "max_runtime_seconds",
    "status",
    "posted_message_count",
    "created_at",
    "updated_at",
    "setup_revision_id",
];

/// The columns the narrative log's rows carry.
const EVENT_SHAPE: &[&str] = &["id", "runner_id", "event_type", "metadata", "created_at"];

/// The columns the report's usage line carries once the biller settled it.
///
/// `fleet_name` is the fleet's name as it stood when the line was written, and
/// `platform_model_price_id` the catalogue row it was priced at — so their
/// presence HERE, on a line the booted daemon wrote through the real report
/// and settle path, proves both reach production rather than merely the
/// statement text. `run_id` belongs to setup chat and stays
/// empty on a fleet line.
const LEDGER_SHAPE: &[&str] = &[
    "id",
    "work_type",
    "work_id",
    "kind",
    "seq",
    "tenant_id",
    "workspace_id",
    "fleet_id",
    "fleet_name",
    "turn_id",
    "provider",
    "model",
    "posture",
    "requested_at",
    "input_token_count",
    "cached_input_token_count",
    "cache_write_token_count",
    "cache_write_5m_token_count",
    "cache_write_1h_token_count",
    "output_token_count",
    "runtime_ms",
    "platform_model_price_id",
    "unpriced_token_count",
    "charged_nanos",
    "settled_at",
    "created_at",
    "updated_at",
];

/// The columns one captured memory entry carries.
///
/// Memory is scoped to a FLEET: `(fleet_id, key)` is both the upsert's conflict
/// target and the fleet's own overwrite mechanism. `workspace_id` rides along
/// so a workspace-visible entry is readable by its siblings, and the capture
/// verb cross-checks it against the run that authorised the write.
const MEMORY_SHAPE: &[&str] = &[
    "id",
    "fleet_id",
    "workspace_id",
    "key",
    "content",
    "category",
    "workspace_visible",
    "created_at",
    "updated_at",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_cycle_settles_in_the_ledger_and_populates_rows() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();
    let run_id = complete_cycle(&http, &scene).await;
    assert_rows(&scene, &run_id).await;
    assert_values(&scene, &run_id).await;
    assert_settled_in_the_ledger(&scene).await;
    supervisor.shutdown().await;
    scene.cleanup().await;
}

async fn complete_cycle(http: &reqwest::Client, scene: &Scenario) -> String {
    let beat = crate::wire::heartbeat(scene, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );

    let (run_id, fence) = wait_for_seeded_assignment(scene).await;

    let captured = post(
        http,
        scene,
        &format!("/v1/runners/me/memory/{}", scene.fleet),
        &json!({
            "run_id": run_id,
            "fencing_token": fence,
            "memory": [{
                "key": MEMORY_KEY,
                "content": MEMORY_CONTENT,
                "category": MEMORY_CATEGORY,
            }],
        }),
    )
    .await;
    assert_eq!(captured.status().as_u16(), 200, "the memory capture lands");

    let settled = crate::wire::report(scene, &report_body(&run_id, &scene.turn_id, fence)).await;
    assert!(settled.is_ok(), "native report refused: {settled:?}");
    run_id
}

async fn assert_rows(scene: &Scenario, run_id: &str) {
    // `settled_at` is part of the recorded line shape; the relay fills it.
    relay_settled(scene).await;
    let run = Uuid7::parse(run_id).expect("the daemon answers a canonical run id");
    assert_shape(scene, "thread.runs", "id = $1", &run, RUN_SHAPE).await;
    assert_shape(
        scene,
        "runner.history",
        "runner_id = $1",
        &scene.runner_id,
        EVENT_SHAPE,
    )
    .await;
    assert_shape(
        scene,
        "billing.usage_lines",
        "id = (SELECT id FROM billing.usage_lines WHERE turn_id = $1 \
         ORDER BY requested_at DESC, id DESC LIMIT 1)",
        &scene.turn_id,
        LEDGER_SHAPE,
    )
    .await;
    assert_shape(
        scene,
        "memory.entries",
        "fleet_id = $1",
        &scene.fleet,
        MEMORY_SHAPE,
    )
    .await;
}

async fn assert_values(scene: &Scenario, run_id: &str) {
    assert_eq!(
        run_column(scene, run_id, "posture").await.as_deref(),
        Some(POSTURE),
        "the run records the posture the money pass resolved"
    );
    assert_eq!(
        run_column(scene, run_id, "model").await.as_deref(),
        Some(MODEL),
        "and the model it was priced at"
    );
    assert_eq!(
        line_output_tokens(scene).await,
        i64::from(OUTPUT_TOKENS),
        "the runner's request lines carried the scene's tokens onto the usage lines"
    );
}

async fn assert_settled_in_the_ledger(scene: &Scenario) {
    let settled: bool = sqlx::query_scalar(
        "SELECT run_fence IS NOT NULL AND opened_at IS NOT NULL AND settled_at IS NOT NULL \
         FROM thread.turns WHERE id = $1 AND thread_id = $2",
    )
    .bind(&scene.turn_id)
    .bind(&scene.fleet)
    .fetch_one(&mut *scene.booted.database.acquire().await.expect("database"))
    .await
    .expect("accepted event remains in the ledger");
    assert!(settled, "the report settled the turn it held");
}
