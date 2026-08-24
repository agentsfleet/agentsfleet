//! The pools themselves: one per role, each with its own limits.
//!
//! # The raw pool does not leave this module
//!
//! Invariant 4 of the milestone: every I/O deadline is a `tokio::time::timeout`
//! at the call site, and that is only enforceable if callers cannot reach a
//! `PgPool` and start their own unbounded operation on it. [`Db`] owns the pool
//! privately and is constructed from a [`PoolConfig`] that carries the
//! timeouts, so there is no way to hold a connection source without also
//! holding its limits.
//!
//! # Connecting probes first, then pools
//!
//! A pool cannot tell you why it failed. `PgPoolOptions::connect_with` retries
//! internally until the acquire timeout runs out and then reports
//! `PoolTimedOut` — the same error a busy pool returns — so a refused port, a
//! wrong password and a genuinely exhausted pool all arrive as one variant.
//! An operator paged with "pool exhausted" goes and raises a limit while the
//! database stays down.
//!
//! So [`Db::connect`] opens ONE connection directly first. That call reports
//! what actually happened (`Io`, `Database`, TLS), and only then is the pool
//! built. `test_pool_error_classes` is the proof, and it failed against a dead
//! port until this existed.

use std::time::Duration;

use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Connection as _, Postgres};

use crate::config::{DbRole, PoolConfig};
use crate::env::EnvSource;
use crate::error::{Error, classify_acquire, unreachable_datastore};

/// One role's connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    role: DbRole,
    pool: PgPool,
    acquire_timeout: Duration,
    /// Kept because `acquire` needs it to tell a full pool from an absent
    /// datastore, and sqlx does not report a pool's configured ceiling back.
    max_connections: u32,
}

impl Db {
    /// Opens the pool for `config`'s role, proving the datastore answers.
    ///
    /// # Errors
    /// Returns a datastore-unavailable error when Postgres refuses, is
    /// unreachable, or does not answer within the connect timeout.
    pub async fn connect(config: &PoolConfig) -> Result<Self, Error> {
        let role = config.role();
        let acquire_timeout = config.acquire_timeout();

        // The probe. Its error is the real one — the pool's would not be.
        // `tokio::time::timeout` at the call site is this milestone's stated
        // shape for a deadline (Invariant 4), and it is needed here because a
        // handshake budget is a different promise from an acquire budget.
        let probe = tokio::time::timeout(
            config.connect_timeout(),
            sqlx::PgConnection::connect_with(&config.connect_options()),
        )
        .await
        .map_err(|_elapsed| {
            unreachable_datastore(role.tag(), config.connect_timeout().as_millis())
        })?
        .map_err(|source| classify_acquire(role.tag(), acquire_timeout.as_millis(), source))?;
        // A failed close on a connection that already answered proves nothing
        // and is not the caller's problem; the socket goes with the value.
        let _ = probe.close().await;

        let mut builder = PgPoolOptions::new()
            .max_connections(config.max_connections())
            .acquire_timeout(acquire_timeout);

        if role == DbRole::Migrator {
            // The backstop for a lock nobody released. Advisory locks live on
            // the SESSION, and a pooled connection outlives the guard that took
            // one — so a task cancelled between acquire and release would hand
            // a still-locked connection back to the pool and every later
            // migrator would wait out its bound against itself. Releasing on
            // return costs one statement per checkin on a pool that sees a
            // handful of them per process lifetime.
            builder = builder.after_release(|connection, _meta| {
                Box::pin(async move {
                    sqlx::query("SELECT pg_advisory_unlock_all()")
                        .execute(connection)
                        .await?;
                    Ok(true)
                })
            });
        }

        // Lazy, because the probe above already proved the datastore answers.
        // A second eager handshake here would only re-ask a question whose
        // answer is one line up.
        let pool = builder.connect_lazy_with(config.connect_options());

        tracing::info!(
            role = role.tag(),
            size = config.max_connections(),
            acquire_timeout_ms = acquire_timeout.as_millis(),
            "pool_initialized"
        );
        Ok(Self {
            role,
            pool,
            acquire_timeout,
            max_connections: config.max_connections(),
        })
    }

    /// Takes a connection out of the pool.
    ///
    /// # Errors
    /// Returns a capacity error when the pool had none free within the acquire
    /// timeout, and a datastore-unavailable error when Postgres itself is the
    /// problem. Those are two different incidents; see [`crate::error`].
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>, Error> {
        self.pool.acquire().await.map_err(|source| {
            let waited_ms = self.acquire_timeout.as_millis();
            // sqlx says `PoolTimedOut` both when every connection is busy and
            // when it could not open a new one at all. The pool's own census
            // separates them: at the ceiling with none free is capacity;
            // BELOW the ceiling and still timing out means the connections it
            // tried to open never came up, which is the datastore.
            if matches!(source, sqlx::Error::PoolTimedOut)
                && self.pool.size() < self.max_connections
            {
                return unreachable_datastore(self.role.tag(), waited_ms);
            }
            classify_acquire(self.role.tag(), waited_ms, source)
        })
    }

    /// The role this pool serves.
    #[must_use]
    pub const fn role(&self) -> DbRole {
        self.role
    }

    /// How long an acquire waits before reporting capacity exhaustion.
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }

    /// Closes the pool and waits for its connections to drain.
    ///
    /// Named rather than left to `Drop`: closing is asynchronous, `Drop` cannot
    /// await, and a pool dropped mid-shutdown abandons connections the server
    /// is still holding open. §7's supervisor calls this in stop order.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// The three pools a daemon runs on.
///
/// Separate roles rather than one pool with three names: the migrator needs a
/// session endpoint (advisory locks do not survive a transaction pooler) and
/// the API role runs with narrower privileges, so a shared pool would silently
/// give request-path queries the migrator's rights.
#[derive(Debug, Clone)]
pub struct Pools {
    default: Db,
    api: Db,
    migrator: Db,
}

impl Pools {
    /// Resolves and opens all three pools from `env`.
    ///
    /// # Errors
    /// Returns the first role's config or connection error, naming the knob or
    /// the role — no role is silently skipped, because a daemon missing one is
    /// a daemon that fails later and further from the cause.
    pub async fn connect_all<E: EnvSource + ?Sized>(env: &E) -> Result<Self, Error> {
        Ok(Self {
            default: Self::open(env, DbRole::Default).await?,
            api: Self::open(env, DbRole::Api).await?,
            migrator: Self::open(env, DbRole::Migrator).await?,
        })
    }

    async fn open<E: EnvSource + ?Sized>(env: &E, role: DbRole) -> Result<Db, Error> {
        Db::connect(&PoolConfig::resolve(env, role)?).await
    }

    /// The pool for background work and anything unscoped.
    #[must_use]
    pub const fn default_role(&self) -> &Db {
        &self.default
    }

    /// The request-path pool.
    #[must_use]
    pub const fn api(&self) -> &Db {
        &self.api
    }

    /// The migration pool. Must be a session endpoint.
    #[must_use]
    pub const fn migrator(&self) -> &Db {
        &self.migrator
    }

    /// The pool for `role`, for callers that carry the role as data.
    #[must_use]
    pub const fn role(&self, role: DbRole) -> &Db {
        match role {
            DbRole::Default => &self.default,
            DbRole::Api => &self.api,
            DbRole::Migrator => &self.migrator,
        }
    }

    /// Closes every pool, in reverse of the order they were opened.
    pub async fn close(&self) {
        self.migrator.close().await;
        self.api.close().await;
        self.default.close().await;
    }
}
