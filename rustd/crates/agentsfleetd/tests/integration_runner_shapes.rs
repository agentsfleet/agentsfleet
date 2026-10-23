//! The daemon populates runtime, usage and memory rows through a full runner cycle.
#![cfg(feature = "test-util")]
#![allow(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{MODEL, POSTURE, Scenario, scenario};
use crate::reads::{assert_shape, lease_column, line_output_tokens};
use crate::wire::{
    MEMORY_CATEGORY, MEMORY_CONTENT, MEMORY_KEY, OUTPUT_TOKENS, capable_beat,
    poll_for_seeded_lease, post, report_body,
};

/// rows, so the report can rebuild the price without re-resolving the tenant.
///
/// `posted_message_count` is `NOT NULL`, so every lease carries it from the
/// moment it is written, and the messages verb counts against it under the
/// lease's fence. The lease names its event by row id (`fleet_event_id`); the
/// actor and type are read from the event, not copied onto the lease.
const LEASE_SHAPE: &[&str] = &[
    "id",
    "runner_id",
    "fleet_id",
    "workspace_id",
    "tenant_id",
    "fleet_event_id",
    "event_created_at",
    "posture",
    "provider",
    "model",
    "last_metered_at",
    "fencing_token",
    "lease_expires_at",
    "status",
    "posted_message_count",
    "created_at",
    "updated_at",
];

/// The columns the narrative log's rows carry.
const EVENT_SHAPE: &[&str] = &["id", "runner_id", "event_type", "metadata", "created_at"];

/// The columns the report's usage line carries once the biller settled it.
///
/// `fleet_name` is the fleet's name as it stood when the line was written, and
/// `platform_model_price_id` the catalogue row it was priced at — so their
/// presence HERE, on a line the booted daemon wrote through the real report
/// and settle path, proves both reach production rather than merely the
/// statement text. `authoring_attempt_id` belongs to setup chat and stays
/// empty on a fleet line.
const LEDGER_SHAPE: &[&str] = &[
    "id",
    "work_type",
    "work_id",
    "seq",
    "tenant_id",
    "workspace_id",
    "fleet_id",
    "fleet_name",
    "event_id",
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
/// verb cross-checks it against the lease that authorised the write.
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
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();
    let lease_id = complete_cycle(&http, &run).await;
    assert_rows(&run, &lease_id).await;
    assert_values(&run, &lease_id).await;
    assert_ledger_only_queue(&run).await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn complete_cycle(http: &reqwest::Client, run: &Scenario) -> String {
    let beat = post(http, run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities"
    );

    let (lease_id, fence) = poll_for_seeded_lease(http, run).await;

    let captured = post(
        http,
        run,
        &format!("/v1/runners/me/memory/{}", run.fleet),
        &json!({
            "lease_id": lease_id,
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

    let settled = post(
        http,
        run,
        "/v1/runners/me/reports",
        &report_body(&lease_id, &run.event_id, fence),
    )
    .await;
    assert_eq!(settled.status().as_u16(), 200, "the report settles");
    lease_id
}

async fn assert_rows(run: &Scenario, lease_id: &str) {
    assert_shape(run, "runner.leases", "id = $1::uuid", lease_id, LEASE_SHAPE).await;
    assert_shape(
        run,
        "runner.events",
        "runner_id = $1::uuid",
        run.runner_id.as_str(),
        EVENT_SHAPE,
    )
    .await;
    assert_shape(
        run,
        "billing.usage_lines",
        "event_id = $1 AND seq = (SELECT max(seq) FROM billing.usage_lines WHERE event_id = $1)",
        &run.event_id,
        LEDGER_SHAPE,
    )
    .await;
    assert_shape(
        run,
        "memory.entries",
        "fleet_id = $1::uuid",
        &run.fleet,
        MEMORY_SHAPE,
    )
    .await;
}

async fn assert_values(run: &Scenario, lease_id: &str) {
    assert_eq!(
        lease_column(run, lease_id, "posture").await.as_deref(),
        Some(POSTURE),
        "the lease records the posture the money pass resolved"
    );
    assert_eq!(
        lease_column(run, lease_id, "model").await.as_deref(),
        Some(MODEL),
        "and the model it was priced at"
    );
    assert_eq!(
        line_output_tokens(run).await,
        i64::from(OUTPUT_TOKENS),
        "the runner's request lines carried the run's tokens onto the usage lines"
    );
}

async fn assert_ledger_only_queue(run: &crate::e2e::Scenario) {
    let queue = afd_dragonfly::test_util::connect_live(&crate::e2e::dragonfly_config())
        .await
        .expect("live cluster");
    for pattern in [
        format!("fleet:{}:events", run.fleet),
        "fleet:ready*".to_owned(),
    ] {
        assert!(
            queue
                .scan_keys(&pattern, 64)
                .await
                .expect("all primaries scanned")
                .is_empty(),
            "a completed cycle created the obsolete queue key {pattern}"
        );
    }
    let settled: bool = sqlx::query_scalar(
        "SELECT lease_fence IS NOT NULL AND delivered_at IS NOT NULL AND settled_at IS NOT NULL \
         FROM fleet.admissions WHERE id = $1::uuid AND fleet_id = $2::uuid",
    )
    .bind(&run.event_id)
    .bind(&run.fleet)
    .fetch_one(&mut *run.booted.database.acquire().await.expect("database"))
    .await
    .expect("accepted event remains in the ledger");
    assert!(settled, "the report settled the admission it leased");
}
