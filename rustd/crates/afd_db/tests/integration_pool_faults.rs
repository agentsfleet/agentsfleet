//! What the pool does when Postgres accepts the socket and then says nothing.
//!
//! `integration_pool.rs` proves the pool works and that a saturated one reports
//! capacity. These prove the other half of that distinction, which is the half
//! an operator is paged by: **a pool that is full and a datastore that is gone
//! must never report as each other.** `PoolTimedOut` is sqlx's answer to both,
//! and `pool.rs` separates them by asking whether the pool was below its own
//! ceiling at the time — below it and still timing out means the connections it
//! tried to open never came up, which is the datastore and not the load.
//!
//! Getting it wrong is expensive in a specific way: "pool capacity" sends
//! someone to raise `DATABASE_POOL_SIZE` on a database that is not answering,
//! which adds connections to a server that cannot take the ones it has.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::net::SocketAddr;
use std::time::Duration;

use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::migration::Migration;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, Migrator};

#[path = "support/fault_net.rs"]
mod fault_net;

use self::fault_net::{FaultProxy, install_subscriber};

const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Short enough that a test waits it out, long enough that a loaded machine
/// does not trip it while the proxy is still relaying normally.
const ACQUIRE_BUDGET_MS: &str = "400";

/// The same, for the handshake the probe makes.
const CONNECT_BUDGET: Duration = Duration::from_millis(400);

/// One migration that would apply cleanly, so the only thing that can go wrong
/// is the transaction the migrator opens to apply it in.
const TRIVIAL: &[Migration] = &[Migration::for_test(
    9101,
    "9101_trivial.sql",
    "CREATE TABLE public.trivial_marker (id int)",
)];

/// The lane's Postgres, as an address a proxy can forward to.
fn lane_target() -> SocketAddr {
    let url = std::env::var(LANE_KNOB).unwrap_or_else(|_| {
        panic!("{LANE_KNOB} is unset — run these through the integration lane")
    });
    let after_scheme = url.split_once("://").expect("a URL has a scheme").1;
    let authority = after_scheme
        .rsplit_once('@')
        .map_or(after_scheme, |(_credentials, host)| host);
    let host_port = authority
        .split_once('/')
        .map_or(authority, |(host, _path)| host);
    let (host, port) = host_port
        .rsplit_once(':')
        .expect("the lane URL names a port");
    // The lane spells this `localhost`, which resolves to both stacks. The
    // proxy binds v4, so the target is pinned to v4 rather than left to
    // whichever the resolver returns first.
    let host = if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    };
    format!("{host}:{port}")
        .parse()
        .expect("the lane's Postgres address must parse")
}

/// The lane's own database, for the faults that only need a socket to die on.
fn lane_database() -> String {
    let url = std::env::var(LANE_KNOB).unwrap_or_else(|_| {
        panic!("{LANE_KNOB} is unset — run these through the integration lane")
    });
    let after_scheme = url.split_once("://").expect("a URL has a scheme").1;
    let path = after_scheme
        .split_once('/')
        .expect("the lane URL names a database")
        .1;
    path.split_once('?')
        .map_or(path, |(database, _query)| database)
        .to_owned()
}

/// A configuration pointed at `addr` instead of the real datastore.
///
/// `database` is a parameter and not the lane's own, because one test below
/// runs a MIGRATOR through the proxy. `Migrator::run` reaps every ledger row
/// below its migration list's floor, and [`TRIVIAL`] sits at 9101 — so pointed
/// at the shared lane database it deletes all forty-seven rows the lane's
/// `_migrate-test-db` just wrote. The schema objects survive that, the ledger
/// does not, and the next `agentsfleetd migrate` replays 810 onto a trigger
/// that already exists. The failure surfaces in `agentsfleetd`, three crates
/// away from the test that caused it.
fn config_through(addr: SocketAddr, role: DbRole, database: &str) -> PoolConfig {
    let url = format!("postgres://agentsfleet:agentsfleet@{addr}/{database}?sslmode=disable");
    let env = MapEnv::from_pairs([
        (role.url_knob(), url.as_str()),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", ACQUIRE_BUDGET_MS),
    ]);
    PoolConfig::resolve(&env, role)
        .expect("the constructed URL must resolve")
        .with_connect_timeout(CONNECT_BUDGET)
}

/// A datastore that accepts the socket and never completes the handshake fails
/// the connect on its own deadline, naming the role.
///
/// The deadline has to be here rather than left to the pool's acquire budget,
/// and that is the point of the branch: a handshake budget and an acquire
/// budget are different promises, and a boot that inherited the acquire one
/// would hold the process open for a multiple of the time it should.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_handshake_that_never_completes_fails_on_its_own_deadline() {
    install_subscriber();
    let proxy = FaultProxy::swallowing(lane_target()).await;

    let started = tokio::time::Instant::now();
    let error = Db::connect(&config_through(proxy.addr(), DbRole::Api, &lane_database()))
        .await
        .expect_err("a handshake that never completes must not succeed");

    assert!(
        error.is_datastore_unavailable(),
        "a server that never answers is unavailable, not a capacity problem: {error}"
    );
    assert!(
        error.to_string().contains("api"),
        "the failure must name the role, since a deployment runs three: {error}"
    );
    // The connect budget bounded it, not the acquire budget and not the lane's
    // timeout. Generous on the upper side: this asserts which budget applied,
    // not how precise a timer is on a loaded machine.
    assert!(
        started.elapsed() < CONNECT_BUDGET * 8,
        "the handshake deadline must be what bounded this, in {:?}",
        started.elapsed()
    );
}

/// A pool that loses the datastore after connecting reports it as unavailable,
/// not as a pool that ran out of connections.
///
/// The pool is below its own ceiling the whole time — it opens connections
/// lazily and never got one — so the census is what tells the two apart. A
/// build that reported capacity here would send an operator to raise the pool
/// size against a Postgres that is not answering.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_losing_the_datastore_after_connect_is_not_reported_as_capacity() {
    install_subscriber();
    let proxy = FaultProxy::to(lane_target()).await;

    // Connects for real: the probe goes through the proxy to live Postgres, so
    // this is a pool that genuinely reached its datastore once.
    let db = Db::connect(&config_through(proxy.addr(), DbRole::Api, &lane_database()))
        .await
        .expect("the proxy relays, so the connect must succeed");

    // And now the datastore stops answering. The pool is lazy, so it holds no
    // connection at this point and every acquire has to open one.
    proxy.swallow();

    let error = db
        .acquire()
        .await
        .expect_err("an acquire that cannot open a connection must fail");

    assert!(
        error.is_datastore_unavailable(),
        "below the ceiling and still timing out is the datastore: {error}"
    );
    assert!(
        !error.is_pool_capacity(),
        "a pool that never opened a connection did not run out of them: {error}"
    );

    db.close().await;
}

/// A migrator whose connection dies at the transaction it is about to open
/// reports the `BEGIN` that failed, not a migration that did.
///
/// The distinction is the whole point of the branch. Nothing has been applied
/// and nothing has failed to apply — the connection went away between the
/// ledger read and the transaction — so reporting a migration failure here
/// would write a failure row for a migration that never ran, and the next boot
/// would report a version as broken when it had simply never been attempted.
///
/// Deterministic by construction rather than by timing. Every statement before
/// this one travels on the same connection and would fail first if the kill
/// were scheduled by a clock; the proxy instead recognises the `BEGIN` on the
/// wire and drops the connection rather than forwarding it, so the failure
/// lands at exactly one call and never earlier.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_connection_that_dies_at_begin_reports_the_transaction() {
    install_subscriber();
    let proxy = FaultProxy::cutting_on(lane_target(), b"BEGIN").await;

    // A database of this test's own: the migrate below reaps, and reaping the
    // shared lane database is what `config_through` documents.
    let database = TestDatabase::create().await;
    let name = database
        .database_name()
        .expect("a created database has a name")
        .to_owned();
    let db = Db::connect(&config_through(proxy.addr(), DbRole::Migrator, &name))
        .await
        .expect("the proxy relays until a BEGIN, so the connect must succeed");

    let error = Migrator::new()
        .with_migrations(TRIVIAL)
        .run(&db)
        .await
        .expect_err("a transaction that cannot be opened must fail the migrate");

    assert!(
        error.is_query(),
        "the BEGIN failed, so this is a query error: {error}"
    );
    assert!(
        !error.is_migration_failed(),
        "no migration ran, so none may be reported as failed: {error}"
    );
    assert!(
        error.to_string().contains("migrate.begin_tx"),
        "the failure must name the operation that produced it: {error}"
    );

    db.close().await;
    database.cleanup().await;
}

/// Warming a pool whose datastore has gone reports the shortfall and boots.
///
/// `warm` is documented as infallible on purpose: a pool that could not reach
/// its floor is a slower pool, not a broken one, and failing boot over a
/// warm-up would trade a cold start for an outage. That promise is only worth
/// anything on the path where the datastore is actually gone — every other
/// warm test has a live Postgres and reaches the floor, so the give-up arm and
/// the shortfall it logs never ran.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_warming_a_pool_whose_datastore_went_away_reports_the_shortfall() {
    install_subscriber();
    let proxy = FaultProxy::to(lane_target()).await;

    let url = format!(
        "postgres://agentsfleet:agentsfleet@{}/{}?sslmode=disable",
        proxy.addr(),
        lane_database()
    );
    // A floor of exactly one: above zero so `warm` asks for something, and no
    // higher, because every extra is another acquire this test waits out while
    // holding a slot on the lane's shared Postgres. `integration_pool.rs`
    // retries its own warm-up for precisely that reason — sibling load is what
    // makes a 250 ms handshake budget lapse on a healthy datastore — so this
    // one keeps its footprint to the single acquire the claim needs.
    let env = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), url.as_str()),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", ACQUIRE_BUDGET_MS),
        ("DATABASE_POOL_SIZE_API", "1"),
        ("DATABASE_MIN_POOL_SIZE_API", "1"),
    ]);
    let config = PoolConfig::resolve(&env, DbRole::Api)
        .expect("the constructed URL must resolve")
        .with_connect_timeout(CONNECT_BUDGET);

    let db = Db::connect(&config)
        .await
        .expect("the proxy relays, so the connect must succeed");
    proxy.swallow();

    let warmed = db.warm(std::time::Duration::from_secs(1)).await;

    assert_eq!(
        warmed, 0,
        "no connection could be opened, so none was warmed"
    );
    assert_eq!(
        db.size(),
        0,
        "and nothing was left behind in the pool either"
    );

    db.close().await;
}
