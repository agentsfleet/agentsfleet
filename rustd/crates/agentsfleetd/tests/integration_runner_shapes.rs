//! Dimension 7.2 — the statements fill the columns they were written to fill.
//!
//! A binary of its own, beside `integration_runner_e2e/mod.rs` (RULE FLL, split by
//! concern): that suite asserts a SEQUENCE — the loop a runner walks — while
//! this one asserts a SHAPE, and the two share only the scenario that produces
//! the rows. Keeping them together meant one file carrying four recorded column
//! sets and a request loop, which is two subjects and one place to look for
//! both.
//!
//! # What this proves
//!
//! Every row carries every column its statement was written to fill. A
//! statement that drops a column from an `INSERT` list still compiles, still
//! returns `Ok`, and still passes every behavioural assertion that reads some
//! OTHER column. This is the test that does not.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![allow(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{MODEL, POSTURE, scenario};
use crate::reads::{assert_shape, lease_column};
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
    "receipt",
    "posture",
    "provider",
    "model",
    "metered_input_token_count",
    "metered_cached_token_count",
    "metered_output_token_count",
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

/// Dimension 7.2 — four tables, one scenario, four recorded shapes.
///
/// Four because those are the four tables Dimension 7.2 names, and they are
/// written by four
/// different statements across three different requests: the lease by the poll,
/// the narrative row alongside it, the memory entry by the capture, and the
/// ledger row by the report.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_seeded_row_shapes() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities"
    );

    let (lease_id, fence) = poll_for_seeded_lease(&http, &run).await;

    let captured = post(
        &http,
        &run,
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
        &http,
        &run,
        "/v1/runners/me/reports",
        &report_body(&lease_id, &run.event_id, fence),
    )
    .await;
    assert_eq!(settled.status().as_u16(), 200, "the report settles");

    assert_shape(
        &run,
        "runner.leases",
        "id = $1::uuid",
        &lease_id,
        LEASE_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "runner.events",
        "runner_id = $1::uuid",
        run.runner_id.as_str(),
        EVENT_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "billing.usage_lines",
        "event_id = $1 AND seq = (SELECT max(seq) FROM billing.usage_lines WHERE event_id = $1)",
        &run.event_id,
        LEDGER_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "memory.entries",
        "fleet_id = $1::uuid",
        &run.fleet,
        MEMORY_SHAPE,
    )
    .await;

    // The values that are NOT identifiers or clock reads, so the shape check
    // above cannot pass over a statement that filled every column with the
    // wrong thing.
    assert_eq!(
        lease_column(&run, &lease_id, "posture").await.as_deref(),
        Some(POSTURE),
        "the lease records the posture the money pass resolved"
    );
    assert_eq!(
        lease_column(&run, &lease_id, "model").await.as_deref(),
        Some(MODEL),
        "and the model it was priced at"
    );
    assert_eq!(
        lease_column(&run, &lease_id, "metered_output_tokens")
            .await
            .as_deref(),
        Some(OUTPUT_TOKENS.to_string().as_str()),
        "the report advanced the lease's own cursor, not only the affinity's"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
