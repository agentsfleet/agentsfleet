//! Dimension 7.3 — a write waiting on a held wallet row gives up within the
//! pool's lock limit, and the pool serves the next request.
//!
//! The wallet's production writers (signup, the lease debit) live in crates
//! that depend on this one, so the waiting write here is the statement shape
//! they issue, sent through the same `Api` pool they use.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::pool::{LOCK_TIMEOUT, STATEMENT_TIMEOUT};
use afd_db::test_util::TestDatabase;
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::lane::Lane;

/// SQLSTATE `lock_not_available`: the wait outlived `lock_timeout`. Class 55
/// is "object not in prerequisite state", which a caller may retry.
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// Holds the wallet row the way a debit in flight does.
const LOCK_WALLET: &str = "SELECT 1 FROM billing.wallets WHERE tenant_id = $1::uuid FOR UPDATE";

/// The write a second debit issues against the same row.
const TOUCH_WALLET: &str = "\
UPDATE billing.wallets SET updated_at = now() WHERE tenant_id = $1::uuid";

/// The slack a lock wait may overrun its limit by before the test calls the
/// limit absent: scheduling and one round trip, never a second wait.
const OVERRUN: Duration = Duration::from_secs(1);

const SHOW_LOCK_TIMEOUT: &str = "SHOW lock_timeout";
const SHOW_STATEMENT_TIMEOUT: &str = "SHOW statement_timeout";

/// Postgres's spelling of a time limit that is off.
const UNLIMITED: &str = "0";

/// The wallet's opening balance; nothing here reads it.
const BALANCE: i64 = 1_000;

/// What a connection on `pool` reports for one setting.
async fn shown(pool: &Db, statement: &'static str) -> String {
    sqlx::query_scalar(statement)
        .fetch_one(&mut *pool.acquire().await.expect("the lane answers"))
        .await
        .expect("the setting reads")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_lock_wait_is_bounded() {
    let lane = Lane::create().await;
    lane.wallet(BALANCE, None, Timestamp::now()).await;

    assert_eq!(shown(&lane.pool, SHOW_LOCK_TIMEOUT).await, "2s");
    assert_eq!(shown(&lane.pool, SHOW_STATEMENT_TIMEOUT).await, "15s");

    let mut held = lane.pool.acquire().await.expect("the holder connects");
    let mut holder = held.begin().await.expect("the holder begins");
    sqlx::query(LOCK_WALLET)
        .bind(lane.tenant.as_str())
        .execute(&mut *holder)
        .await
        .expect("the holder locks the wallet");

    let started = Instant::now();
    let refused = sqlx::query(TOUCH_WALLET)
        .bind(lane.tenant.as_str())
        .execute(&mut *lane.pool.acquire().await.expect("a second connection"))
        .await
        .expect_err("a write behind a held row must give up, not wait");
    let waited = started.elapsed();

    let code = refused
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .map(String::from);
    assert_eq!(code.as_deref(), Some(LOCK_NOT_AVAILABLE), "{refused}");
    assert!(waited >= LOCK_TIMEOUT, "gave up after {waited:?}");
    assert!(waited < LOCK_TIMEOUT + OVERRUN, "waited {waited:?}");
    assert!(waited < STATEMENT_TIMEOUT);

    holder.rollback().await.expect("the holder lets go");
    let touched = sqlx::query(TOUCH_WALLET)
        .bind(lane.tenant.as_str())
        .execute(&mut *lane.pool.acquire().await.expect("the pool still serves"))
        .await
        .expect("the next write goes through");
    assert_eq!(touched.rows_affected(), 1);
}

/// A migration waits on locks and runs long statements by design, so its
/// connection carries neither limit.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_migrator_connection_carries_no_time_limits() {
    let migrator = TestDatabase::shared().open(DbRole::Migrator, &[]).await;
    assert_eq!(shown(&migrator, SHOW_LOCK_TIMEOUT).await, UNLIMITED);
    assert_eq!(shown(&migrator, SHOW_STATEMENT_TIMEOUT).await, UNLIMITED);
}
