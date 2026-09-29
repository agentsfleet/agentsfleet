//! A pool on a Postgres nobody answers on, for the suites that prove what a
//! failure arm does when the datastore is gone.
//!
//! One spelling of the address and the budget, so every caller refuses the
//! same way: the transport class, in milliseconds, with no socket opened until
//! the first acquire asks for one.

use afd_core::env::MapEnv;

use crate::config::{ACQUIRE_TIMEOUT_KNOB, DbRole, PoolConfig};
use crate::pool::Db;

/// A Postgres nobody listens on: port 1 is reserved and unbound.
const NOWHERE_DATABASE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A short acquire budget, so a refused acquire costs milliseconds.
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// A pool whose every acquire fails as the transport class.
///
/// # Panics
/// When this module's own address or budget is malformed: a fixture fault
/// that should stop the suite.
#[must_use]
pub fn unreachable_db() -> Db {
    let env = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE_DATABASE),
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    let config = PoolConfig::resolve(&env, DbRole::Api).unwrap_or_else(|failure| {
        panic!("the unreachable fixture's URL is well-formed: {failure}")
    });
    Db::unreachable(&config)
}
