//! Taking a connection out of the pool, and what a failed take is called.
//!
//! # Three incidents, not two
//!
//! `pool.rs` separates a full pool from an absent datastore by asking whether
//! the pool was below its ceiling when sqlx reported `PoolTimedOut`. Below the
//! ceiling used to be read as "the datastore is gone", and that reading was
//! wrong in production: one instance answered 503 twelve times in seven
//! minutes while its sibling ran hundreds of queries against the same
//! Postgres in the same minute. What "below the ceiling and timed out"
//! actually proves is narrower — the connections sqlx tried to open did not
//! come up inside the budget. And sqlx's connect loop retries a refused socket
//! and a `53300 too_many_connections` SILENTLY until that budget runs out
//! (`sqlx-core/src/pool/inner.rs`, `PoolInner::connect`), so a server that is
//! up and refusing one more backend arrives here as the same `PoolTimedOut`
//! as a server that is down. That is a stall. It is named as one, and it
//! carries the census that says so, because the operator's next move is to
//! read the server's connection ceiling, not to page for an outage.
//!
//! # One retry, on the stall alone
//!
//! A stall is the one failure a second attempt can change: the burst that
//! needed a connection has thinned, the server has reaped a backend it was
//! still counting, the handshake that overran has finished. Capacity is not
//! retried — every connection is busy, and waiting again is a longer acquire
//! timeout by another name. A refused or failed handshake is not retried
//! either: sqlx already reported what happened, and a second try would only
//! delay that answer.

use sqlx::Postgres;
use sqlx::pool::PoolConnection;

use super::Db;
use crate::error::{Result, acquire_stalled, classify_acquire};
use afd_core::error_code;

impl Db {
    /// Takes a connection out of the pool.
    ///
    /// # Errors
    /// Returns a capacity error when the pool was at its ceiling with none free
    /// within the acquire timeout; a stall when it was below the ceiling and
    /// could not open one, after a single retry; and a datastore-unavailable
    /// error when Postgres itself refused. Three different incidents; see
    /// [`crate::error`].
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>> {
        match self.acquire_once().await {
            Err(error) if error.is_acquire_stalled() => {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let code = error_code::INTERNAL_DB_UNAVAILABLE.as_str();
                let role = self.role.tag();
                let held = self.pool.size();
                let ceiling = self.max_connections;
                let waited_ms = self.acquire_timeout.as_millis();
                tracing::warn!(
                    error_code = code,
                    role,
                    held,
                    ceiling,
                    waited_ms,
                    event = "pool_acquire_retried"
                );
                self.acquire_once().await
            }
            first => first,
        }
    }

    /// One attempt, classified by the pool's own census.
    ///
    /// sqlx says `PoolTimedOut` both when every connection is busy and when it
    /// could not open a new one at all. At the ceiling with none free is
    /// capacity; BELOW the ceiling and still timing out is the stall the
    /// module documentation describes, and nothing more specific than that can
    /// be claimed from here.
    async fn acquire_once(&self) -> Result<PoolConnection<Postgres>> {
        self.pool.acquire().await.map_err(|source| {
            let waited_ms = self.acquire_timeout.as_millis();
            let held = self.pool.size();
            if matches!(source, sqlx::Error::PoolTimedOut) && held < self.max_connections {
                return acquire_stalled(self.role.tag(), waited_ms, held, self.max_connections);
            }
            classify_acquire(self.role.tag(), waited_ms, source)
        })
    }
}
