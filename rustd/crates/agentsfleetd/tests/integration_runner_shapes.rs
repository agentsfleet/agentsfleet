//! Dimension 7.2 — the ported statements fill the columns they were written
//! to fill.
//!
//! A binary of its own, beside `integration_runner_e2e.rs` (RULE FLL, split by
//! concern): that suite asserts a SEQUENCE — the loop a runner walks — while
//! this one asserts a SHAPE, and the two share only the scenario that produces
//! the rows. Keeping them together meant one file carrying four recorded column
//! sets and a request loop, which is two subjects and one place to look for
//! both.
//!
//! # What this replaces
//!
//! §7 records the trade: M175 §6 deleted the Zig integration lanes, so no second
//! daemon exists to diff rows against. This is the weaker claim that stands in
//! its place — not "these rows equal the Zig's", which nothing can check any
//! more, but "these rows carry every column the ported statement was written to
//! fill". A port that drops a column from an `INSERT` list still compiles, still
//! returns `Ok`, and still passes every behavioural assertion that reads some
//! OTHER column. This is the test that does not.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

#[path = "support/e2e_db.rs"]
mod e2e_db;

#[path = "support/e2e_seed.rs"]
mod e2e_seed;

#[path = "support/e2e.rs"]
mod e2e;

#[path = "support/e2e_wire.rs"]
mod wire;

#[path = "support/e2e_reads.rs"]
mod reads;

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use self::e2e::{MODEL, POSTURE, scenario};
use self::reads::{assert_shape, lease_column};
use self::wire::{
    MEMORY_CATEGORY, MEMORY_CONTENT, MEMORY_KEY, OUTPUT_TOKENS, capable_beat, claim, json, post,
    report_body,
};

/// rows, so the report can rebuild the price without re-resolving the tenant.
const LEASE_SHAPE: &[&str] = &[
    "id",
    "runner_id",
    "fleet_id",
    "workspace_id",
    "tenant_id",
    "event_id",
    "actor",
    "event_type",
    "event_created_at",
    "posture",
    "provider",
    "model",
    "metered_input_tokens",
    "metered_cached_tokens",
    "metered_output_tokens",
    "last_metered_at",
    "fencing_token",
    "lease_expires_at",
    "status",
    "created_at",
    "updated_at",
];

/// The columns the narrative log's rows carry.
const EVENT_SHAPE: &[&str] = &["id", "runner_id", "event_type", "metadata", "created_at"];

/// The columns a `stage` ledger row carries after a settle.
const LEDGER_SHAPE: &[&str] = &[
    "id",
    "tenant_id",
    "workspace_id",
    "fleet_id",
    "event_id",
    "charge_type",
    "posture",
    "model",
    "credit_deducted_nanos",
    "token_count_input",
    "token_count_cached_input",
    "token_count_output",
    "wall_ms",
    "event_created_at",
    "created_at",
    "last_charged_at",
];

/// The columns one captured memory entry carries.
///
/// No `workspace_id`, and the table has none: memory is scoped to a FLEET, and
/// `(key, fleet_id)` is both the upsert's conflict target and the fleet's own
/// overwrite mechanism. The write is authorised by a lease whose workspace the
/// capture verb cross-checks, so the scope is enforced on the way in rather
/// than denormalised onto every row.
const MEMORY_SHAPE: &[&str] = &[
    "id",
    "key",
    "content",
    "category",
    "fleet_id",
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
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_seeded_row_shapes() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities"
    );

    let body = json(post(&http, &run, "/v1/runners/me/leases", &json!({})).await).await;
    let lease = body
        .get("lease")
        .filter(|value| !value.is_null())
        .expect("the seeded fleet is leasable");
    let (lease_id, fence) = claim(lease);

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
        "fleet.runner_leases",
        "id = $1::uuid",
        &lease_id,
        LEASE_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "fleet.runner_events",
        "runner_id = $1::uuid",
        run.runner_id.as_str(),
        EVENT_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "billing.usage_ledger",
        "event_id = $1 AND charge_type = 'stage'",
        &run.event_id,
        LEDGER_SHAPE,
    )
    .await;
    assert_shape(
        &run,
        "memory.memory_entries",
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
