//! The integration lane's Postgres, as the fault suites need to address it.
//!
//! Split from `integration_pool_faults.rs` when a second suite needed the
//! same three answers: where the lane's Postgres listens, which database is
//! the lane's own, and how to point a role's configuration at a proxy that is
//! standing in front of it.

#![allow(
    dead_code,
    reason = "support module: `#[path]`-included into more than one test file, each of \
              which asks a different subset of these questions"
)]

use std::net::SocketAddr;
use std::time::Duration;

use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};

pub(crate) const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Short enough that a test waits it out, long enough that a loaded machine
/// does not trip it while the proxy is still relaying normally.
pub(crate) const ACQUIRE_BUDGET_MS: u64 = 400;

/// The same, for the handshake the probe makes.
pub(crate) const CONNECT_BUDGET: Duration = Duration::from_millis(ACQUIRE_BUDGET_MS);

/// The lane's Postgres, as an address a proxy can forward to.
pub(crate) fn lane_target() -> SocketAddr {
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
pub(crate) fn lane_database() -> String {
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
pub(crate) fn config_through(addr: SocketAddr, role: DbRole, database: &str) -> PoolConfig {
    let url = format!("postgres://agentsfleet:agentsfleet@{addr}/{database}?sslmode=disable");
    let env = MapEnv::from_pairs([
        (role.url_knob(), url.as_str()),
        (
            "DATABASE_ACQUIRE_TIMEOUT_MS",
            &ACQUIRE_BUDGET_MS.to_string(),
        ),
    ]);
    PoolConfig::resolve(&env, role)
        .expect("the constructed URL must resolve")
        .with_connect_timeout(CONNECT_BUDGET)
}
