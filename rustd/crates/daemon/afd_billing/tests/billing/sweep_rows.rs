//! The sweep's row handling, over a database of this suite's own.
//!
//! The shared lane's sweep reads every unsettled line in it, so a foreign or
//! corrupt row seeded there would reach every other suite's pass. These checks
//! build a private database holding only the usage log's columns.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code;
use afd_billing::biller::RecordingBiller;
use afd_billing::ledger::{resettle, unsettled};
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

/// One unsettled line, logged at the epoch.
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

/// A line of a work type this build does not write is skipped by the read, so
/// the pass settles nothing and one foreign row cannot stall the relay.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_foreign_line_is_skipped_by_the_read_and_the_pass() {
    let (lane, database) = private_log().await;
    seed(&database, FOREIGN_WORK, None).await;

    let mut connection = database.acquire().await.expect("a private connection");
    let (read, _resume) = unsettled(&mut *connection, READ_LIMIT, None)
        .await
        .expect("the log reads");
    assert!(
        read.is_empty(),
        "a foreign line opens no settlement: {read:?}"
    );
    drop(connection);

    let biller = RecordingBiller::new();
    let swept = resettle(&database, &biller, None)
        .await
        .expect("the pass reads");
    assert_eq!(swept.settled, 0, "{swept:?}");
    assert!(biller.settlements().is_empty(), "nothing was settled");

    database.close().await;
    lane.cleanup().await;
}

/// What is still owed is read after every pass; a read that cannot sum it
/// fails the pass rather than publishing a silent zero.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unreadable_debt_fails_the_pass() {
    let (lane, database) = private_log().await;
    for _ in 0..2 {
        seed(&database, afd_billing::sql::work::FLEET_LEASE, None).await;
    }
    sqlx::query("UPDATE billing.usage_lines SET charged_nanos = 9223372036854775807")
        .execute(&mut *database.acquire().await.expect("a private connection"))
        .await
        .expect("two lines no BIGINT can sum");

    let biller = RecordingBiller::new();
    let failed = resettle(&database, &biller, None)
        .await
        .expect_err("the owed read overflows");
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
    let refused = unsettled(&mut *connection, READ_LIMIT, None)
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

/// A full batch the relay cannot move does not hold newer lines behind it: the
/// read resumes after the last line it returned, and a read that reaches the
/// newest line answers no resume point, so the next read starts a new lap.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_read_resumes_after_a_full_batch_and_then_laps() {
    const BATCH: i64 = 3;
    let (lane, database) = private_log().await;
    for _ in 0..=BATCH {
        seed(&database, afd_billing::sql::work::FLEET_LEASE, None).await;
    }

    let mut connection = database.acquire().await.expect("a private connection");
    let (first, resume) = unsettled(&mut *connection, BATCH, None)
        .await
        .expect("the first batch reads");
    assert_eq!(first.len(), 3, "a full batch: {first:?}");
    let resume = resume.expect("a full batch says where to resume");
    let (rest, lap) = unsettled(&mut *connection, BATCH, Some(&resume))
        .await
        .expect("the next batch reads");
    assert_eq!(rest.len(), 1, "the line past the stuck batch is reached");
    let reached = rest.first().expect("one line past the batch");
    assert!(
        first.iter().all(|work| work.work != reached.work),
        "the resumed read returns no line the first one did"
    );
    assert!(lap.is_none(), "reaching the newest line starts a new lap");
    drop(connection);

    database.close().await;
    lane.cleanup().await;
}
