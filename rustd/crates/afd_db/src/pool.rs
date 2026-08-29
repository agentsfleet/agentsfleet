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
use crate::error::{Result, classify_acquire, unreachable_datastore};
use afd_core::env::EnvSource;

/// One role's connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    role: DbRole,
    pool: PgPool,
    acquire_timeout: Duration,
    /// Kept because `acquire` needs it to tell a full pool from an absent
    /// datastore, and sqlx does not report a pool's configured ceiling back.
    max_connections: u32,
    /// Kept because [`Self::warm`] establishes this floor itself; sqlx cannot
    /// bootstrap it on a lazy pool, and does not report it back either.
    min_connections: u32,
}

impl Db {
    /// Opens the pool for `config`'s role, proving the datastore answers.
    ///
    /// # Errors
    /// Returns a datastore-unavailable error when Postgres refuses, is
    /// unreachable, or does not answer within the connect timeout.
    pub async fn connect(config: &PoolConfig) -> Result<Self> {
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
            // The floor sqlx maintains in the background. Without it the pool
            // opens its first connection inside the first request, and an
            // establishment measured at 147-337 ms lands inside an acquire
            // budget that was sized for a wait, not a handshake.
            .min_connections(config.min_connections())
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

        // Lazy, and deliberately so after measuring the alternative.
        //
        // `connect_with` establishes `min_connections` before returning, which
        // is what a warm pool wants — but sqlx hands it `acquire_timeout` as
        // its deadline, the same number that bounds how long one REQUEST waits,
        // and it opens them one after another. Twelve connections at the
        // 147-337 ms each this lane measures do not fit in two seconds under
        // load, and `Db::connect` failed outright — a process refusing to start
        // where it previously started and warmed as it went.
        //
        // So the pool opens empty and [`Db::warm`] fills it afterwards, on a
        // budget of its own. What is NOT relied on is sqlx
        // filling it: the background bootstrap runs only when `max_lifetime`
        // and `idle_timeout` are both `None`, and leaving them unset is not
        // that — the defaults are `Some`, which buys the reaper this pool wants
        // and costs the one-shot warm-up it would otherwise have had.
        let pool = builder.connect_lazy_with(config.connect_options());

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let role_tag = role.tag();
        let max_connections = config.max_connections();
        let min_connections = config.min_connections();
        let acquire_timeout_ms = acquire_timeout.as_millis();
        tracing::info!(
            role = role_tag,
            size = max_connections,
            warm = min_connections,
            acquire_timeout_ms,
            event = "pool_initialized"
        );
        Ok(Self {
            role,
            pool,
            acquire_timeout,
            max_connections,
            min_connections,
        })
    }

    /// A pool over a datastore that has NOT been proven to answer.
    ///
    /// Behind `test-util` because production must never hold one: the probe in
    /// [`Db::connect`] is the promise that a boot which returned has a
    /// reachable Postgres, and a constructor that skips it would let a binary
    /// start against a database that is not there.
    ///
    /// What it exists for is the other half of that promise — proving what the
    /// REQUEST path does when the datastore is gone. Every acquire through it
    /// fails, so a suite can drive a real router, through the real handler,
    /// into the transport-class refusal, with no datastore anywhere near the
    /// test. That refusal is the one an authentication failure must never be
    /// confused with (RULE ECL), which makes it worth a seam of its own.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn unreachable(config: &PoolConfig) -> Self {
        Self {
            role: config.role(),
            pool: PgPoolOptions::new()
                .max_connections(config.max_connections())
                .acquire_timeout(config.acquire_timeout())
                .connect_lazy_with(config.connect_options()),
            acquire_timeout: config.acquire_timeout(),
            max_connections: config.max_connections(),
            min_connections: config.min_connections(),
        }
    }

    /// Takes a connection out of the pool.
    ///
    /// # Errors
    /// Returns a capacity error when the pool had none free within the acquire
    /// timeout, and a datastore-unavailable error when Postgres itself is the
    /// problem. Those are two different incidents; see [`crate::error`].
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>> {
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

    /// Opens connections up to the configured floor, before traffic needs them.
    ///
    /// # By holding them, not by releasing them
    ///
    /// Acquiring in a loop warms exactly ONE connection: each acquire returns
    /// the connection the previous iteration just released, and the pool never
    /// has a reason to open a second. The floor is only reached by holding
    /// every connection at once, which is what forces the pool to open one more
    /// each time — so this gathers the guards and drops them together.
    ///
    /// # On a budget of its own
    ///
    /// `deadline` is the caller's, and it is deliberately not `acquire_timeout`:
    /// that one bounds a single request's wait, and warming is a boot activity
    /// whose cost is `floor × establishment`. Every individual acquire is still
    /// bounded by `acquire_timeout` underneath, so a hung server cannot make
    /// this outlast its own deadline by much.
    ///
    /// # Errors
    /// Never. A pool that could not reach its floor is a slower pool, not a
    /// broken one — the datastore's reachability was already proven by the
    /// probe in [`Self::connect`], and failing boot over a warm-up would trade
    /// the cold start for an outage. The shortfall is returned so the caller
    /// can report it, and logged here so it is visible without one.
    pub async fn warm(&self, deadline: Duration) -> u32 {
        let target = self.min_connections;
        if target == 0 {
            return 0;
        }

        let held = tokio::time::timeout(deadline, async {
            let mut connections = Vec::with_capacity(target as usize);
            // Sequential requests for connections that are all held: the pool
            // has to open a new one each time, because none is ever returned.
            for _ in 0..target {
                match self.pool.acquire().await {
                    Ok(connection) => connections.push(connection),
                    Err(_) => break,
                }
            }
            connections
        })
        .await
        .map_or(0, |connections| {
            let opened = connections.len();
            drop(connections);
            u32::try_from(opened).unwrap_or(target)
        });

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let role_tag = self.role.tag();
        let deadline_ms = deadline.as_millis();
        if held < target {
            tracing::warn!(
                role = role_tag,
                warmed = held,
                target,
                deadline_ms,
                event = "pool_warm_incomplete"
            );
        } else {
            tracing::info!(
                role = role_tag,
                warmed = held,
                deadline_ms,
                event = "pool_warmed"
            );
        }
        held
    }

    /// How many connections the pool currently holds open, idle or checked out.
    ///
    /// The census sqlx keeps, exposed so a test can prove [`Self::warm`]
    /// actually opened something. A configured floor and an established one are
    /// different claims, and only this one can tell them apart.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.pool.size()
    }

    /// The floor [`Self::warm`] targets.
    ///
    /// Exposed for the same reason as [`Self::size`]: a caller that warms can
    /// then say whether the floor was REACHED, and the two numbers are the
    /// whole of that claim.
    #[must_use]
    pub const fn min_connections(&self) -> u32 {
        self.min_connections
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
    pub async fn connect_all<E: EnvSource + ?Sized>(env: &E) -> Result<Self> {
        Ok(Self {
            default: Self::open(env, DbRole::Default).await?,
            api: Self::open(env, DbRole::Api).await?,
            migrator: Self::open(env, DbRole::Migrator).await?,
        })
    }

    async fn open<E: EnvSource + ?Sized>(env: &E, role: DbRole) -> Result<Db> {
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
