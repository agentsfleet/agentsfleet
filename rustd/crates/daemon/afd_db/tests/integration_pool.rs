//! Dimension 2.4 — an exhausted pool and an absent datastore, told apart.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::env::MapEnv;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::test_util::TestDatabase;

/// How many times the warm-up below may lose its race before the lane is
/// declared unreachable.
///
/// Three, because the failure it absorbs is a handshake overrunning a 250 ms
/// budget under concurrent load, and a lane that cannot complete one in three
/// tries is not slow, it is down.
const WARMING_ATTEMPTS: usize = 3;

/// Dimension 2.4 — an exhausted pool and an absent datastore are two different
/// answers, because they are two different incidents.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_pool_error_classes() {
    // `shared`, not `create`. `TestDatabase::create` makes an empty,
    // UNMIGRATED database and `test_util` reserves it for the suites whose
    // subject IS schema state — the migrator's ledger, its lock, its failure
    // paths. This suite is about a pool's capacity, which needs a database and
    // not a virgin one, and `CREATE DATABASE` measures 166 ms against the lane's
    // Postgres. Paying that here bought nothing and cost the test its budget:
    // once the crate's suites were aggregated they ran concurrently, a fresh
    // connection under sibling `CREATE DATABASE` load measured 147-337 ms, and
    // the 250 ms below is spent before the pool this test is about is even
    // reached. Widening the budget would have hidden that; taking the lane's
    // database removes it.
    let database = TestDatabase::shared();

    // One connection, a quarter-second to wait for it.     // Holding the only
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
    // Establish the connection BEFORE the timed window, not inside it. The pool
    // is lazy, so the first acquire pays a handshake — measured at 147-337 ms
    // against this lane's Postgres — while the 250 ms budget under test bounds
    // the WAIT for a free connection, not the making of one. Under load the two
    // overlapped: the first acquire timed out and the pool reported a datastore
    // outage while Postgres was answering normally.
    //
    // Bounded, and it cannot mask a real outage. An unreachable Postgres fails
    // every attempt and the assertion below still fires; what the retry absorbs
    // is a handshake that lost a race with sibling load, which is the only way
    // this acquire fails on a healthy lane.
    let mut warming = Err("no attempt was made".to_owned());
    for _attempt in 0..WARMING_ATTEMPTS {
        match db.acquire().await {
            Ok(connection) => {
                warming = Ok(connection);
                break;
            }
            Err(error) => warming = Err(error.to_string()),
        }
    }
    let held = warming
        .expect("the lane's Postgres must hand out one connection within the warm-up attempts");
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

/// A closed pool refuses rather than hanging, which is what makes shutdown
/// ordering observable instead of a race nobody can see.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_closed_pool_refuses_rather_than_hanging() {
    // `shared`, not `create`: this suite is about a pool's behaviour, which
    // needs a database and not a virgin one, and `CREATE DATABASE` under the
    // lane's concurrent suites costs more than the budget below.
    let database = TestDatabase::shared();
    let config = PoolConfig::resolve(&database.env(&[]), DbRole::Api)
        .expect("the lane's environment resolves the api role");
    let db = Db::connect(&config)
        .await
        .expect("the api role opens from the lane's environment");

    assert_eq!(db.role(), DbRole::Api);
    db.acquire()
        .await
        .expect("an open pool serves a connection");

    db.close().await;
    let error = db.acquire().await.expect_err("a closed pool must refuse");
    assert!(
        !error.is_pool_capacity(),
        "a closed pool is not a busy one: {error}"
    );

    database.cleanup().await;
}

/// Dimension 2.4 — the warm floor is real, not merely configured.
///
/// The test this suite did not have. `min_connections` was passed to sqlx and
/// believed; sqlx bootstraps that floor from zero only when `max_lifetime` and
/// `idle_timeout` are both `None`, its defaults set both, and every other arm
/// reaches the floor through an idle reaper whose loop body runs `num_idle()`
/// times — zero, forever, on a pool nothing has opened. The knob established
/// nothing and no test could tell, because the only assertion was arithmetic
/// on the configured number.
///
/// So this one asks the pool. `size()` is sqlx's count of connections it has
/// actually opened, and after `warm` it must have reached the floor.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_warm_floor_is_established_not_merely_configured() {
    const FLOOR: &str = "4";
    const CEILING: &str = "8";

    let database = TestDatabase::shared();
    let db = database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", CEILING),
                ("DATABASE_MIN_POOL_SIZE_API", FLOOR),
            ],
        )
        .await;

    assert_eq!(
        db.size(),
        0,
        "a lazy pool opens nothing until something asks it to"
    );

    let warmed = db.warm(Duration::from_secs(10)).await;

    assert_eq!(warmed, 4, "the whole floor was established");
    assert!(
        db.size() >= 4,
        "and the pool holds them: sqlx reports {} open",
        db.size()
    );
}

/// A floor of zero asks for nothing and waits for nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_zero_floor_warms_nothing() {
    let database = TestDatabase::shared();
    let db = database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "4"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await;

    assert_eq!(db.warm(Duration::from_secs(10)).await, 0);
    assert_eq!(db.size(), 0, "nothing was opened");
}

/// Held connections, pending requests and cancellation are observations of the real pool.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_pool_snapshot_tracks_use_and_cancelled_waiters() {
    let database = TestDatabase::shared();
    let db = database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_ACQUIRE_TIMEOUT_MS_API", "5000"),
            ],
        )
        .await;
    let held = db
        .acquire()
        .await
        .expect("the real pool opens a connection");
    let observed = db.snapshot().expect("an open pool has a reading");
    assert_eq!(
        (
            observed.open,
            observed.idle,
            observed.capacity,
            observed.waiting
        ),
        (1, 0, 1, 0)
    );
    let waiting = db.clone();
    let request = tokio::spawn(async move { waiting.acquire().await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while db.snapshot().unwrap().waiting == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the request waits behind the held connection");
    assert_eq!(db.snapshot().unwrap().waiting, 1);
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(db.snapshot().unwrap().waiting, 0);
    drop(held);
    tokio::time::timeout(Duration::from_secs(2), async {
        while db.snapshot().unwrap().idle != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("SQLx returns the held connection");
    assert_eq!(db.snapshot().unwrap().open, 1);
    db.close().await;
    assert_eq!(db.snapshot(), None);
}
