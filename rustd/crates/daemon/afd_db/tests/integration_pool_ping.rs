//! M245 §8: a checkout tests a connection only after it sat idle.
//!
//! sqlx's default pings every checkout, one round trip per statement a chat
//! sends. The pools ping only a connection idle for `PING_AFTER_IDLE` (1 s) or
//! longer, so a backend lost inside that second fails one statement, and one
//! lost before it is replaced unseen.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;

/// The window the pool skips its ping inside (`afd_db::pool::PING_AFTER_IDLE`).
const PING_AFTER_IDLE: Duration = Duration::from_secs(1);

/// How long the pool is left alone before the idle checkout: past the window
/// by a margin a loaded lane's scheduling cannot eat.
const IDLE_PAST_WINDOW: Duration = Duration::from_millis(1_500);

/// How long the release's own ping may take before the test gives up on it.
const RELEASE_PATIENCE: Duration = Duration::from_secs(5);

const PINGS: &str = "agentsfleet_db_pool_checkout_pings_total";

/// A pool of one connection, so every checkout is the same backend until one
/// is lost.
async fn pool_of_one(database: &TestDatabase) -> Db {
    database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await
}

/// The backend serving the pool's one connection, once it sits idle again.
///
/// sqlx pings a connection as it goes back, in a task of its own. Killing the
/// backend before that ping lands would have the release discard it, and the
/// next checkout would dial fresh for a reason the test is not about.
async fn idle_backend(db: &Db) -> i32 {
    let pid: i32 = {
        let mut connection = db.acquire().await.expect("the pool hands out its one");
        sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *connection)
            .await
            .expect("the backend answers")
    };
    let deadline = Instant::now() + RELEASE_PATIENCE;
    while db.snapshot().is_none_or(|pool| pool.idle == 0) {
        assert!(
            Instant::now() < deadline,
            "the connection never went back idle"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    pid
}

/// Terminates `pid` from outside the pool, and waits until it has gone.
async fn terminate(database: &TestDatabase, pid: i32) {
    let observer = database.open(DbRole::Default, &[]).await;
    let gone: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1, 5000)")
        .bind(pid)
        .fetch_one(&mut *observer.acquire().await.expect("an observer connection"))
        .await
        .expect("the terminate answers");
    assert!(gone, "backend {pid} did not exit inside five seconds");
    observer.close().await;
}

/// One statement on a fresh checkout, with the backend that ran it.
async fn statement(db: &Db) -> Result<i32, sqlx::Error> {
    let mut connection = db.acquire().await.expect("the pool hands out a connection");
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
}

// Value: protects=a checkout inside the window sends no ping; fails_when=the
// pool still pings every checkout, which answers the first statement from a
// fresh dial; why_new=the ping moved behind an idle gate; seam=none;
// no_claim=which error text sqlx reports for a dead socket.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn backend_lost_inside_the_window_fails_one_statement() {
    let database = TestDatabase::shared();
    let db = pool_of_one(&database).await;
    let lost = idle_backend(&db).await;
    let released = Instant::now();

    terminate(&database, lost).await;
    let first = statement(&db).await;
    assert!(
        released.elapsed() < PING_AFTER_IDLE,
        "the lane took {:?} to kill and check out, past the window under test",
        released.elapsed()
    );
    let second = statement(&db).await;

    assert!(
        first.is_err(),
        "an untested checkout runs on the dead backend: {first:?}"
    );
    let replaced = second.expect("the next checkout dials a new backend");
    assert_ne!(replaced, lost, "the dead connection was discarded");
    db.close().await;
}

// Value: protects=a connection idle past the window is tested and replaced, and
// a busy pool pings nothing; fails_when=the gate never pings, or pings every
// checkout; why_new=the counter is new and the gate decides it; seam=none;
// no_claim=the ping's own latency.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn idle_connection_is_pinged_and_replaced() {
    let metrics = af_observability::test_util::Capture::install();
    let api = [("role", "api")];
    let database = TestDatabase::shared();
    let db = pool_of_one(&database).await;
    let lost = idle_backend(&db).await;
    terminate(&database, lost).await;
    tokio::time::sleep(IDLE_PAST_WINDOW).await;
    let before = metrics.sum(PINGS, &api);

    let replaced = statement(&db)
        .await
        .expect("the ping found the dead backend and the checkout dialled");
    let after_idle = metrics.sum(PINGS, &api);
    for _ in 0..100 {
        statement(&db)
            .await
            .expect("a busy pool's connection answers");
    }
    let after_busy = metrics.sum(PINGS, &api);

    assert_ne!(replaced, lost, "the checkout handed out a new backend");
    assert_eq!(after_idle - before, 1, "one idle checkout, one ping");
    assert_eq!(
        after_busy, after_idle,
        "back-to-back checkouts ping nothing"
    );
    db.close().await;
}
