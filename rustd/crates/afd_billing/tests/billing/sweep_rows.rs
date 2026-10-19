//! The sweep's row handling, over a database of this suite's own.
//!
//! The shared lane's sweep reads every unsettled line in it, so a foreign or
//! corrupt row seeded there would reach every other suite's pass. These checks
//! build a private database holding only the usage log's columns, and no
//! `billing.holds`, so the owed read after a pass fails on its own.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::biller::RecordingBiller;
use afd_billing::ledger::{resettle, unsettled};
use afd_core::error_code;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};

/// The usage log's columns the sweep reads, and nothing it does not.
const USAGE_LOG: &str = "CREATE SCHEMA billing; \
    CREATE TABLE billing.usage_lines ( \
      id bigserial PRIMARY KEY, work_type text NOT NULL, work_id uuid NOT NULL, \
      seq bigint NOT NULL, tenant_id uuid NOT NULL, workspace_id uuid NOT NULL, \
      fleet_id uuid, fleet_name text, event_id text, provider text NOT NULL, \
      model text NOT NULL, posture text NOT NULL, requested_at timestamptz NOT NULL, \
      input_token_count bigint NOT NULL, cached_input_token_count bigint NOT NULL, \
      cache_write_token_count bigint NOT NULL, cache_write_5m_token_count bigint NOT NULL, \
      cache_write_1h_token_count bigint NOT NULL, output_token_count bigint NOT NULL, \
      runtime_ms bigint NOT NULL, platform_model_price_id text, \
      unpriced_token_count bigint NOT NULL, charged_nanos bigint NOT NULL, \
      settled_at timestamptz, created_at timestamptz NOT NULL)";

/// One unsettled line logged at the epoch, long past the grace.
const SEED_LINE: &str = "INSERT INTO billing.usage_lines \
    (work_type, work_id, seq, tenant_id, workspace_id, provider, model, posture, \
     requested_at, input_token_count, cached_input_token_count, cache_write_token_count, \
     cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms, \
     platform_model_price_id, unpriced_token_count, charged_nanos, created_at) \
    VALUES ($1, $2::uuid, 0, $3::uuid, $4::uuid, 'sweep', 'sweep', 'platform', \
            'epoch', 0, 0, 0, 0, 0, 0, 0, $5, 0, 0, 'epoch')";

/// A work type no build of this daemon writes.
const FOREIGN_WORK: &str = "foreign_work";

/// Lines one direct read of the log takes.
const READ_LIMIT: i64 = 10;

/// A private database holding the usage log, and a pool on it.
async fn private_log() -> (TestDatabase, Db) {
    let lane = TestDatabase::create().await;
    let database = lane.open(DbRole::Default, &[]).await;
    let mut connection = database.acquire().await.expect("a private connection");
    sqlx::raw_sql(USAGE_LOG)
        .execute(&mut *connection)
        .await
        .expect("the private usage log builds");
    drop(connection);
    (lane, database)
}

async fn seed(database: &Db, work_type: &str, price: Option<&str>) {
    let mut connection = database.acquire().await.expect("a private connection");
    sqlx::query(SEED_LINE)
        .bind(work_type)
        .bind(mint_id())
        .bind(mint_id())
        .bind(mint_id())
        .bind(price)
        .execute(&mut *connection)
        .await
        .expect("the line seeds");
}

/// A line of a work type this build does not write is skipped, so the pass
/// settles nothing; the owed read after it fails, and that failure is the
/// pass's answer rather than a silent zero.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_foreign_line_is_skipped_and_an_unread_debt_fails_the_pass() {
    let (lane, database) = private_log().await;
    seed(&database, FOREIGN_WORK, None).await;

    let mut connection = database.acquire().await.expect("a private connection");
    let read = unsettled(&mut *connection, std::time::Duration::ZERO, READ_LIMIT)
        .await
        .expect("the log reads");
    assert!(
        read.is_empty(),
        "a foreign line opens no settlement: {read:?}"
    );
    drop(connection);

    let biller = RecordingBiller::new();
    let failed = resettle(&database, &biller)
        .await
        .expect_err("the owed read has no holds table to read");
    assert_eq!(failed.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        failed.to_string().contains("read what is still owed"),
        "the failure names the owed read: {failed}"
    );
    assert!(biller.settlements().is_empty(), "nothing was settled");

    database.close().await;
    lane.cleanup().await;
}

/// A line whose price reference is not an identifier refuses the read and
/// names the column, rather than settling at a price nobody can trace.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_malformed_price_reference_refuses_the_read() {
    let (lane, database) = private_log().await;
    seed(
        &database,
        afd_billing::sql::work::FLEET_LEASE,
        Some("not-a-price-id"),
    )
    .await;

    let mut connection = database.acquire().await.expect("a private connection");
    let refused = unsettled(&mut *connection, std::time::Duration::ZERO, READ_LIMIT)
        .await
        .expect_err("a malformed price reference refuses");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        refused
            .to_string()
            .contains("billing.usage_lines.platform_model_price_id"),
        "the refusal names the column: {refused}"
    );
    drop(connection);

    database.close().await;
    lane.cleanup().await;
}
