//! The budget gate's read of a fleet's usage, held to its index.
//!
//! The renew path runs [`SELECT_FLEET_SPEND`] for every live run every few
//! seconds. The claim worth pinning is the PLAN, not the index definition: a
//! definition test stays green against a plan that stopped using it.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::limits::SELECT_FLEET_SPEND;
use sqlx::Row as _;

use crate::queue;
use crate::report_seed::held;

/// The index the spend read must still be served by.
const FLEET_INDEX: &str = "idx_usage_lines_fleet_id_created_at";

/// How many decoy lines the plan probe seeds, and across how many fleets.
///
/// Enough that `workspace_id` alone stops being selective: one fleet per
/// workspace is a fixture artefact, not the production shape.
const DECOY_LINES: i32 = 600;
/// The number of distinct fleets those lines are spread over.
const DECOY_FLEETS: i32 = 60;

/// The spend read reaches the fleet index, never a sequential scan.
///
/// The probe seeds a realistic spread first, inside a transaction that rolls
/// back; `ANALYZE` is not transactional, so it runs again afterwards rather
/// than leaving the planner believing in rows that no longer exist.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_fleet_spend_read_is_served_by_its_index() {
    let held = held().await;
    let workspace: String = {
        let mut connection = held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        sqlx::query("SELECT workspace_id::text FROM fleet.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the fixture fleet is readable")
            .try_get(0)
            .expect("workspace_id decodes as text")
    };
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("the planner probe opens");
    sqlx::query(
        "INSERT INTO billing.usage_lines
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
            provider, model, posture, requested_at, input_token_count, cached_input_token_count,
            cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
            output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at,
            updated_at)
         SELECT ('0199c900-0000-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid, $4,
                ('0199c902-0000-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid, 0,
                $1::uuid, $2::uuid,
                ('0199c901-0000-7000-8000-' || lpad(to_hex(g % $3), 12, '0'))::uuid,
                'decoy', 'spend-probe-' || g, 'anthropic', 'claude-opus-5', 'platform', now(),
                0, 0, 0, 0, 0, 0, 0, 0, 0, now(), now()
         FROM generate_series(1, $5) g",
    )
    .bind(&held.tenant)
    .bind(&workspace)
    .bind(DECOY_FLEETS)
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(DECOY_LINES)
    .execute(&mut *connection)
    .await
    .expect("the decoy lines insert");
    sqlx::query("ANALYZE billing.usage_lines")
        .execute(&mut *connection)
        .await
        .expect("the planner sees the seeded spread");

    // The statement is this workspace's own constant with a keyword in front
    // of it, never input — which is what `AssertSqlSafe` asks the caller to
    // say out loud.
    let explained = sqlx::query(sqlx::AssertSqlSafe(format!("EXPLAIN {SELECT_FLEET_SPEND}")))
        .bind(&workspace)
        .bind(&held.fleet)
        .bind(afd_core::timing::DAY)
        .fetch_all(&mut *connection)
        .await
        .expect("the spend read is explainable");
    let plan: String = explained
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect::<Vec<_>>()
        .join("\n");

    for statement in ["ROLLBACK", "ANALYZE billing.usage_lines"] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the planner probe cleans up after itself");
    }
    drop(connection);

    assert!(
        plan.contains(FLEET_INDEX),
        "the spend read no longer reaches {FLEET_INDEX}:\n{plan}"
    );
    assert!(
        !plan.contains("Seq Scan on usage_lines"),
        "the spend read fell back to a sequential scan, which grows with every \
         line ever written:\n{plan}"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
