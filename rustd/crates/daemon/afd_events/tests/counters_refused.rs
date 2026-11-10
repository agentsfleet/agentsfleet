//! The counters read that could not answer, and what a publisher does with it.
//!
//! No datastore, deliberately: the pool points at a reserved port and refuses
//! in microseconds, which is the exact shape of the outage the best-effort
//! read exists for. The claim is the absence — the frame goes out with NO
//! figures, never with zeros — and a stub that answered zeros would pass a
//! test that only checked the frame still went out.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::env::MapEnv;
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use afd_db::{Db, DbRole, PoolConfig};
use afd_events::{
    fleet_counters, fleet_counters_best_effort, fleet_counters_best_effort_on, fleet_counters_on,
};

/// A connection string nothing listens on.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// The acquire budget knob a deployment sets, cut so the refusal is prompt.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// Short, but not so short the pool gives up before its first connect
/// attempt returns — see `afd_api`'s readiness harness for the reasoning.
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// The fleet the read is asked about; nothing answers, so any id will do.
const FLEET_ID: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";

/// [`FLEET_ID`] as the identifier the read takes.
fn fleet() -> Uuid7 {
    Uuid7::parse(FLEET_ID).expect("the fixture fleet id is a canonical UUIDv7")
}

/// A pool that refuses every acquire.
fn refusing_pool() -> Db {
    let environment = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE),
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    Db::unreachable(
        &PoolConfig::resolve(&environment, DbRole::Api)
            .expect("the fixture connection string is well formed"),
    )
}

/// A read the pool refused answers `None` — the frame goes out without its
/// figures, and a client leaves what it has standing. Zeros here would tell
/// every tile the fleet has done nothing.
#[tokio::test]
async fn a_refused_read_sends_the_frame_without_its_figures() {
    let counters = fleet_counters_best_effort(&refusing_pool(), &fleet()).await;
    assert_eq!(counters, None);
}

/// The fallible read reports the datastore, not a malformed row: the two
/// remedies differ, and a caller that wants the error gets the right one.
#[tokio::test]
async fn a_refused_read_reports_the_datastore() {
    let error = fleet_counters(&refusing_pool(), &fleet())
        .await
        .expect_err("nothing listens on the fixture port");
    assert!(
        error.is_pool_unavailable(),
        "the pool's refusal is reported as the datastore's, got {error:?}"
    );
}

/// The refusal is logged under its registry code, with a `reason` that does
/// not repeat it and carries no captured backtrace.
#[tokio::test]
async fn an_unread_counter_reason_carries_no_code_prefix() {
    let capture = Capture::install();

    let counters = fleet_counters_best_effort(&refusing_pool(), &fleet()).await;

    assert_eq!(counters, None);
    let logged = capture.only("fleet_counters_unread");
    assert!(logged.field("error_code").is_some(), "{logged:?}");
    assert_eq!(logged.field("fleet_id"), Some(FLEET_ID));
    logged.assert_uncoded_reason();
}

/// A statement the database refuses — here one sent inside a transaction an
/// earlier statement already aborted — is reported as the query it was, not
/// as the pool's outage, and the best-effort read still answers no figures
/// rather than zeros.
///
/// An aborted transaction rather than a malformed id: the identifier is a
/// `Uuid7`, so a spelling Postgres would refuse can no longer reach it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_statement_the_database_refuses_reports_the_query() {
    let lane = afd_db::test_util::TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let mut connection = database.acquire().await.expect("a lane connection");
    let mut transaction = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("a transaction opens");
    sqlx::query("SELECT 1 / 0")
        .execute(&mut *transaction)
        .await
        .expect_err("the division aborts the transaction");

    let error = fleet_counters_on(&mut transaction, &fleet())
        .await
        .expect_err("an aborted transaction refuses the read");
    assert_eq!(error.code(), af_core::error_code::INTERNAL_DB_QUERY);
    assert!(!error.is_pool_unavailable(), "{error:?}");
    assert_eq!(
        fleet_counters_best_effort_on(&mut transaction, &fleet()).await,
        None
    );

    drop(transaction);
    drop(connection);
    drop(database);
    lane.cleanup().await;
}
