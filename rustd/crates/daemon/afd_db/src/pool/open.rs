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

use super::observed::pool_role;
use super::{IDLE_TIMEOUT, MAX_LIFETIME, PING_AFTER_IDLE};

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
    let _closed: Result<(), sqlx::Error> = probe.close().await;
    Ok(())
}

/// The options `config`'s pool is built with: its ceiling, its warm floor, its
/// acquire budget, the pinned reaper, the idle-gated checkout ping, and — for
/// the migrator alone — the advisory-lock backstop.
pub(super) fn options(config: &PoolConfig) -> PgPoolOptions {
    let role = config.role();
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
        .max_lifetime(MAX_LIFETIME)
        // The ping moves into `before_acquire`, gated on idle time: see
        // [`PING_AFTER_IDLE`]. A failed ping answers `Err`, which sqlx treats
        // as its own ping failing: the connection closes hard and the checkout
        // takes or opens another.
        .test_before_acquire(false)
        .before_acquire(move |connection, meta| {
            Box::pin(async move {
                if !due_for_ping(meta.idle_for) {
                    return Ok(true);
                }
                af_observability::producers::database::pinged(pool_role(role));
                connection.ping().await?;
                Ok(true)
            })
        });
    if role != DbRole::Migrator {
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

/// Whether a checkout tests a connection that sat idle for `idle_for`.
fn due_for_ping(idle_for: std::time::Duration) -> bool {
    idle_for >= PING_AFTER_IDLE
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{PING_AFTER_IDLE, due_for_ping};

    #[test]
    fn a_connection_used_inside_the_window_goes_out_untested() {
        assert!(!due_for_ping(Duration::ZERO));
        assert!(!due_for_ping(
            PING_AFTER_IDLE.saturating_sub(Duration::from_millis(1))
        ));
    }

    #[test]
    fn a_connection_idle_for_the_window_is_tested() {
        assert!(due_for_ping(PING_AFTER_IDLE));
        assert!(due_for_ping(PING_AFTER_IDLE * 60));
    }
}
