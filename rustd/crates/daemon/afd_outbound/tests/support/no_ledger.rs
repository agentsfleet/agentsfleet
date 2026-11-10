//! A ledger handle whose every statement fails.
//!
//! The lane suites drive claimed jobs straight into the lanes and a poster
//! that answers without a vendor, so the only statements they reach are the
//! ledger writes after a verdict: the attempt count, the stamp, the release.
//! Each fails against this handle, which is what `lane_sharing.rs` grades: a
//! stamp that cannot be recorded must not make the lane deliver twice. A lazy,
//! unreachable pool satisfies the type without opening a connection, and `sqlx`
//! registers a lazy pool with the reactor without dialling anything.
//!
//! Its own module rather than a copy in each suite; the claim suites open a
//! real database instead.

use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};

/// A pool that resolves and never answers.
pub(crate) fn no_ledger() -> Db {
    let environment = af_core::env::MapEnv::from_pairs([(
        DbRole::Api.url_knob(),
        "postgres://nowhere/agentsfleet",
    )]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api).expect("a lazy pool config resolves");
    Db::unreachable(&pool)
}
