//! What the fixture instance reports about ITSELF, and the pool that answers
//! nothing.
//!
//! Split from the builder beside it: `mod.rs` assembles a `Fleet` out of real
//! stores, and this is the readiness half — what `/readyz` is told, and the
//! Postgres configuration that points at a port nothing listens on. That pool
//! and the unreachable bus are the whole reason a router suite needs no
//! datastore: every store below is the PRODUCTION one, over handles that refuse
//! at the first acquire with the error its own crate raises.

use af_core::env::MapEnv;
use afd_api::router::{Dependencies, ReadyInputs};
use afd_db::{DbRole, PoolConfig};

use super::Fleet;

/// A Postgres nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so an
/// acquire fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// acquire budgets.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// A GitHub nobody is listening on, for the same reason and on the same port.
///
/// The library onboarding fetches a bundle before it writes one, and this
/// harness holds the ONLY seam that decides where from — so without it a
/// router-tier test of the onboarding route leaves the machine and asks the
/// real `api.github.com`, which makes the suite depend on a network, a rate
/// limit, and a repository staying public. What it is meant to prove is that
/// the route reaches its pipeline, and a refused connect proves exactly that.
pub(super) const NOWHERE_GITHUB: &str = "http://127.0.0.1:1";

/// The Slack an interim message is posted to in a router suite: nowhere, for
/// the reason [`NOWHERE_GITHUB`] is.
pub(super) const NOWHERE_SLACK: &str = NOWHERE_GITHUB;

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
