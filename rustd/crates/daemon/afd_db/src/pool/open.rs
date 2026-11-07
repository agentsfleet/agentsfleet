//! The two steps [`Db::connect`](super::Db::connect) takes before it holds a
//! pool: one direct connection that reports the real failure, then the options
//! the pool itself is built from.
//!
//! Split from [`super`] so the constructor reads as its sequence — probe,
//! build, open lazily, report — with each step's reasoning beside the step
//! rather than inside one long body. The parent's module note explains why the
//! probe comes first.

use sqlx::Connection as _;
use sqlx::postgres::PgPoolOptions;

use crate::config::{DbRole, PoolConfig};
use crate::error::{Result, classify_acquire, unreachable_datastore};

use super::{IDLE_TIMEOUT, MAX_LIFETIME};

/// Drops every advisory lock a returned connection still holds.
const UNLOCK_ALL: &str = include_str!("sql/unlock_all.sql");

/// Opens and closes ONE connection, so a refused port, a wrong password and a
/// TLS fault each report as themselves rather than as a pool timeout.
///
/// `tokio::time::timeout` at the call site is this milestone's stated shape
/// for a deadline (Invariant 4), and it is needed here because a handshake
/// budget is a different promise from an acquire budget.
pub(super) async fn probe(config: &PoolConfig) -> Result<()> {
    let role = config.role();
    let probe = tokio::time::timeout(
        config.connect_timeout(),
        sqlx::PgConnection::connect_with(&config.connect_options()),
    )
    .await
    .map_err(|_elapsed| unreachable_datastore(role.tag(), config.connect_timeout()))?
    .map_err(|source| classify_acquire(role.tag(), config.acquire_timeout(), source))?;
    // A failed close on a connection that already answered proves nothing
    // and is not the caller's problem; the socket goes with the value.
    let _ = probe.close().await;
    Ok(())
}

/// The options `config`'s pool is built with: its ceiling, its warm floor, its
/// acquire budget, the pinned reaper, and — for the migrator alone — the
/// advisory-lock backstop.
pub(super) fn options(config: &PoolConfig) -> PgPoolOptions {
    let builder = PgPoolOptions::new()
        .max_connections(config.max_connections())
        // The floor sqlx maintains in the background. Without it the pool
        // opens its first connection inside the first request, and an
        // establishment measured at 147-337 ms lands inside an acquire
        // budget that was sized for a wait, not a handshake.
        .min_connections(config.min_connections())
        .acquire_timeout(config.acquire_timeout())
        // Pinned, not defaulted: see the constants for what turns on them.
        .idle_timeout(IDLE_TIMEOUT)
        .max_lifetime(MAX_LIFETIME);
    if config.role() != DbRole::Migrator {
        return builder;
    }
    // The backstop for a lock nobody released. Advisory locks live on the
    // SESSION, and a pooled connection outlives the guard that took one — so a
    // task cancelled between acquire and release would hand a still-locked
    // connection back to the pool and every later migrator would wait out its
    // bound against itself. Releasing on return costs one statement per
    // checkin on a pool that sees a handful of them per process lifetime.
    builder.after_release(|connection, _meta| {
        Box::pin(async move {
            sqlx::query(UNLOCK_ALL).execute(connection).await?;
            Ok(true)
        })
    })
}
