//! A ledger handle a suite never reaches.
//!
//! The lane and backoff suites drive a FAKE queue and a poster that answers
//! without a vendor, so no delivery ever reaches the obligation stamp. A lazy,
//! unreachable pool is the honest handle to give them: it satisfies the type
//! without opening a connection, and `sqlx` registers a lazy pool with the
//! reactor without dialling anything.
//!
//! Its own module rather than a copy in each suite, and deliberately NOT part
//! of `outbound_harness` — that harness opens a real database, which is exactly
//! what these two suites exist without.

use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};

/// A pool that resolves and never answers.
pub(crate) fn no_ledger() -> Db {
    let environment = afd_core::env::MapEnv::from_pairs([(
        DbRole::Api.url_knob(),
        "postgres://nowhere/agentsfleet",
    )]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api).expect("a lazy pool config resolves");
    Db::unreachable(&pool)
}
