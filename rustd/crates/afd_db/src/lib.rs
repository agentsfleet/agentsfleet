//! Postgres for `agentsfleetd`: one pool per role, and the schema migrator.
//!
//! # The ledger is shared, so the port cannot invent its own
//!
//! Whichever binary a deploy runs, it migrates the same database. That makes
//! `audit.schema_migrations`, the version numbers in it, and the advisory key
//! the lock is taken under a DATA FORMAT rather than an implementation detail
//! — which is why `sqlx`'s own migrator is not enabled here. It keeps a
//! different ledger under a different lock, and two migrators disagreeing
//! about what is applied is the one failure this module exists to prevent.
//!
//! What the Rust side does differently is everything that is not the format:
//! versions are derived from filenames during constant evaluation rather than
//! restated beside them ([`migration`]), an unterminated migration cannot be
//! turned into a statement list at all ([`sql`]), the reap list is a bind
//! rather than a rendered `IN` clause ([`migrate::ledger`]), and the lock owns
//! the session that holds it ([`migrate::lock`]).
//!
//! # Two acquire failures, kept apart
//!
//! A pool that timed out and a datastore that is gone are one wire code and
//! two incidents. [`Error::is_pool_capacity`] and
//! [`Error::is_datastore_unavailable`] answer them separately, because the
//! operator's next move differs and the client's does not.

// Same reasoning as afd_crypto: an unused dependency is supply-chain surface
// and compile time for nothing, and this is the level the check has to sit at
// because a dev-dependency legitimately goes unused by the library itself.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
// Every duplicate in this crate's graph is inside sqlx's, not ours:
// `cargo tree -d` puts sha2 0.10.9, digest 0.10.7, block-buffer 0.10.4 and
// crypto-common 0.1.7 under sqlx-core, and getrandom 0.2.17 under
// ring → rustls → sqlx-core. This workspace's own pins are the current line
// (aes-gcm 0.11.1, sha2 0.11.0, hmac 0.13.0, getrandom 0.4.3), so there is
// nothing here to unify — the fix is upstream in sqlx or nowhere. Re-check
// when sqlx bumps: this is `expect`, so it fails the build once it stops
// being true rather than sitting here forever.
#![expect(
    clippy::multiple_crate_versions,
    reason = "sqlx-core pins the RustCrypto 0.10 line and ring's getrandom 0.2; this workspace is on the current line"
)]

pub mod config;
pub mod env;
pub mod error;
pub mod migrate;
pub mod migration;
pub mod pool;
pub mod sql;

pub use crate::config::{DbRole, PoolConfig};
pub use crate::error::Error;
pub use crate::migrate::{Applied, Migrator};
pub use crate::migration::{MIGRATIONS, Migration};
pub use crate::pool::{Db, Pools};

/// The knob that decides whether `serve` migrates before it listens.
///
/// Read here rather than in the boot path so the spelling lives beside the
/// migrator it governs; §7 is what calls it.
pub const MIGRATE_ON_START_KNOB: &str = "MIGRATE_ON_START";

/// Whether this deployment migrates at boot.
///
/// Absent is false: a deployment that wants schema changes applied by whatever
/// container happens to start first has to say so, because the alternative —
/// defaulting to true — makes every replica a migrator during a rolling
/// restart.
///
/// # Errors
/// Returns a config error when the value is neither truthy nor falsy.
/// `MIGRATE_ON_START=yes` is not "no"; it is an operator who believes
/// migrations are on, and `cmd/common.zig:44-48` refuses it for that reason.
pub fn migrate_on_start<E: env::EnvSource + ?Sized>(source: &E) -> Result<bool, Error> {
    let Some(raw) = source.get(MIGRATE_ON_START_KNOB) else {
        return Ok(false);
    };
    match config::parse_env_bool(&raw) {
        config::EnvBool::Yes => Ok(true),
        config::EnvBool::No => Ok(false),
        config::EnvBool::Invalid => Err(error::invalid_bool_knob(MIGRATE_ON_START_KNOB)),
    }
}
