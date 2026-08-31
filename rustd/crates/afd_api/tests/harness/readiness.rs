//! What the fixture instance reports about ITSELF, and the two handles that
//! answer nothing.
//!
//! Split from the builder beside it: `mod.rs` assembles a `Fleet` out of real
//! stores, and this is the readiness half — what `/readyz` is told, and the
//! Postgres and Redis configurations that point at a port nothing listens on.
//! Those two are the whole reason a router suite needs no datastore: every
//! store below is the PRODUCTION one, over handles that refuse at the first
//! acquire with the error its own crate raises.

use afd_api::router::{Dependencies, ReadyInputs};
use afd_core::env::MapEnv;
use afd_db::{DbRole, PoolConfig};
use afd_redis::{RedisConfig, RedisRole};

use super::Fleet;

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A Redis nobody is listening on, for the same reason and on the same port.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The pool knob naming how long an acquire may spend before it reports.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// What this harness sets it to — see [`unreachable_pool`].
const ACQUIRE_TIMEOUT_MS: &str = "50";

impl Dependencies for Fleet {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        std::future::ready(self.ready)
    }
}

/// A Postgres configuration pointed at an address that answers nothing.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
pub(super) fn unreachable_pool() -> PoolConfig {
    let environment = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE),
        // The acquire budget, cut from the two-second production default.
        //
        // Every request in this harness ends at a refused connection, and the
        // pool spends the whole budget retrying before it reports one. At the
        // default that is two seconds per request and roughly ten per suite —
        // paid on every inner-loop run, to learn something the first
        // millisecond already knew.
        //
        // Set through the SAME knob a deployment sets, not through a test-only
        // constructor, so what the suite configures is what an operator can. It
        // must not go so low that the pool gives up before its first connect
        // attempt returns: `sqlx` reports that as `PoolTimedOut`, which
        // `afd_db` classifies as pool CAPACITY rather than an unreachable
        // datastore, and the refusal would change class. A refused TCP connect
        // on a reserved port answers in microseconds, so this has three orders
        // of magnitude of headroom — and if it ever stops having them, the
        // assertions on `DATABASE_UNAVAILABLE` fail loudly rather than drifting.
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    PoolConfig::resolve(&environment, DbRole::Api)
        .expect("the fixture connection string is well formed")
}

/// The same, for the queue the login surface and the fleet install reach.
pub(super) fn unreachable_queue() -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, NOWHERE_QUEUE.to_owned())
        .with_request_timeout(std::time::Duration::from_millis(250))
}
