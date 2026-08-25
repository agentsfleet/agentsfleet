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

/// A configuration pointed at `addr` instead of the real datastore.
fn config_through(addr: SocketAddr, role: DbRole) -> PoolConfig {
    let url = format!("postgres://agentsfleet:agentsfleet@{addr}/agentsfleetdb?sslmode=disable");
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
    let error = Db::connect(&config_through(proxy.addr(), DbRole::Api))
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
    let db = Db::connect(&config_through(proxy.addr(), DbRole::Api))
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

    let db = Db::connect(&config_through(proxy.addr(), DbRole::Migrator))
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
}
