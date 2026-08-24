//! Dimension 2.4 — an exhausted pool and an absent datastore, told apart.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::env::MapEnv;

#[path = "support/test_database.rs"]
mod support;

use self::support::TestDatabase;

/// Dimension 2.4 — an exhausted pool and an absent datastore are two different
/// answers, because they are two different incidents.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_pool_error_classes() {
    let database = TestDatabase::create().await;

    // One connection, a quarter-second to wait for it. Holding the only
    // connection makes the next acquire a capacity failure and nothing else —
    // Postgres is up and answering the whole time.
    let db = database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_ACQUIRE_TIMEOUT_MS_API", "250"),
            ],
        )
        .await;
    let held = db.acquire().await.expect("the first connection is free");
    let error = db
        .acquire()
        .await
        .expect_err("a one-connection pool has nothing left to give");
    assert!(
        error.is_pool_capacity(),
        "an exhausted pool is a capacity incident, not an outage: {error}"
    );
    assert!(
        !error.is_datastore_unavailable(),
        "the datastore was up the whole time: {error}"
    );
    assert_eq!(error.code().as_str(), "UZ-INTERNAL-001");
    drop(held);
    db.close().await;

    // Nothing listens on port 1. This is the other incident, and it must not
    // arrive as a capacity error — an operator paging on "pool exhausted"
    // would go and raise a limit while the database stays down.
    let env = MapEnv::from_pairs([(
        "DATABASE_URL",
        "postgres://agentsfleet:agentsfleet@127.0.0.1:1/agentsfleetdb?sslmode=disable",
    )]);
    let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
    let error = Db::connect(&config)
        .await
        .expect_err("nothing is listening on port 1");
    assert!(
        error.is_datastore_unavailable(),
        "an unreachable datastore is an outage: {error}"
    );
    assert!(!error.is_pool_capacity(), "got {error}");

    database.cleanup().await;
}
