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

use afd_core::env::MapEnv;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};

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

/// All three pools open from one environment, and each answers as its own role.
///
/// `Pools` is what boot actually calls; the single-role path every other test
/// uses would let a wiring mistake here — two roles sharing one pool, or
/// `role()` dispatching to the wrong field — reach production untested.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_pools_open_every_role_and_close() {
    let database = TestDatabase::create().await;
    let pools = afd_db::Pools::connect_all(&database.env(&[]))
        .await
        .expect("every role must open from one environment");

    assert_eq!(pools.default_role().role(), DbRole::Default);
    assert_eq!(pools.api().role(), DbRole::Api);
    assert_eq!(pools.migrator().role(), DbRole::Migrator);

    // The role-as-data lookup must agree with the named accessors, or a caller
    // that carries its role in a variable talks to a different database than
    // one that names it.
    for role in DbRole::ALL {
        assert_eq!(pools.role(*role).role(), *role, "{role:?} dispatched wrong");
        pools
            .role(*role)
            .acquire()
            .await
            .expect("each role must serve a connection");
    }
    assert_eq!(
        pools.api().acquire_timeout(),
        std::time::Duration::from_millis(2_000)
    );

    pools.close().await;
    // A closed pool refuses rather than hanging, which is what makes shutdown
    // ordering observable instead of a race nobody can see.
    let error = pools
        .api()
        .acquire()
        .await
        .expect_err("a closed pool must refuse");
    assert!(
        !error.is_pool_capacity(),
        "a closed pool is not a busy one: {error}"
    );

    database.cleanup().await;
}
