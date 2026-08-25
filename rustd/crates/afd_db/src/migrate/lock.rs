//! The advisory lock that makes two migrators safe, and the bound on waiting.
//!
//! # Why the wait is bounded
//!
//! `pg_advisory_lock` waits forever. A migrator that crashed while holding the
//! lock leaves the next deploy hanging until the deploy machine's own timeout
//! fires — minutes later, with no output saying why. So this polls
//! `pg_try_advisory_lock` a bounded number of times and then fails loudly with
//! a named error, which puts "lock held" in front of an operator in seconds.
//! The bound IS the stop path (`pool_migration_lock.zig:6-13`).
//!
//! # Why a query error is not a retry
//!
//! A dropped connection is not contention, and polling does not fix it. It
//! propagates immediately rather than burning the whole bound first and
//! reporting the wrong cause thirty seconds late.

use std::time::Duration;

use sqlx::pool::PoolConnection;
use sqlx::{Executor as _, Postgres, Row as _};

use afd_core::error_code;

use crate::error::{Error, ErrorKind, Result, query};

/// The operations a failure names, one spelling each (RULE UFS).
const OP_ACQUIRE: &str = "migrate.acquire_lock";
const OP_PROBE: &str = "migrate.probe_lock";

/// The one key the schema migration lock is taken under, cluster-wide.
///
/// Same constant as `pool_migration_lock.zig:29`, because the Zig daemon and
/// this binary must contend with each other rather than migrate in parallel.
const ADVISORY_KEY: i64 = 0x7A6F_6D62_6965_0001;

/// How long to keep polling a held lock before giving up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: u32,
    interval: Duration,
}

impl RetryPolicy {
    /// The production bound: thirty polls a second apart, so a held lock is
    /// reported in about thirty seconds rather than never.
    pub const PRODUCTION: Self = Self {
        attempts: 30,
        interval: Duration::from_secs(1),
    };

    /// A bound a test can wait out, for the contention proofs.
    #[must_use]
    pub const fn new(attempts: u32, interval: Duration) -> Self {
        Self { attempts, interval }
    }

    /// The longest this policy can wait, which is what the failure reports.
    #[must_use]
    pub const fn budget(&self) -> Duration {
        self.interval.saturating_mul(self.attempts)
    }
}

/// One poll's verdict.
///
/// A named decision rather than a condition inline in the loop: it is the part
/// worth testing without a database, and `pool_migration_lock.zig` splits it
/// out for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    /// This session now holds the lock.
    Acquired,
    /// Someone else holds it and there are polls left.
    Retry,
    /// Someone else holds it and the bound is spent.
    Exhausted,
}

/// Classifies a single poll. Pure, so the bound is provable without a database.
#[must_use]
pub const fn classify(acquired: bool, attempt: u32, policy: RetryPolicy) -> Attempt {
    if acquired {
        Attempt::Acquired
    } else if attempt >= policy.attempts {
        Attempt::Exhausted
    } else {
        Attempt::Retry
    }
}

/// A held migration lock, owning the session that holds it.
///
/// Advisory locks are session-scoped, so the connection and the lock are one
/// thing and this type says so: there is no way to hold the lock without
/// holding the connection it lives on, and [`MigrationLock::release`] consumes
/// the guard so it cannot be released twice.
#[derive(Debug)]
pub struct MigrationLock {
    connection: PoolConnection<Postgres>,
}

impl MigrationLock {
    /// Polls for the lock until it is held or the policy is spent.
    ///
    /// # Errors
    /// Returns a migration-refused error when the bound is exhausted, and a
    /// query error when the poll itself fails — a dropped connection is not
    /// contention and is not retried.
    pub async fn acquire(
        mut connection: PoolConnection<Postgres>,
        policy: RetryPolicy,
    ) -> Result<Self> {
        for attempt in 1..=policy.attempts {
            let acquired: bool = sqlx::query("SELECT pg_try_advisory_lock($1)")
                .bind(ADVISORY_KEY)
                .fetch_one(&mut *connection)
                .await
                .map_err(|source| query(OP_ACQUIRE, source))?
                .try_get(0)
                .map_err(|source| query(OP_ACQUIRE, source))?;

            match classify(acquired, attempt, policy) {
                Attempt::Acquired => {
                    if attempt > 1 {
                        tracing::info!(attempt, "migrate_lock_acquired_after_contention");
                    }
                    return Ok(Self { connection });
                }
                Attempt::Retry => {
                    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                    let retry_ms = policy.interval.as_millis();
                    tracing::warn!(
                        attempt,
                        max_attempts = policy.attempts,
                        retry_ms,
                        "migrate_lock_contended"
                    );
                    tokio::time::sleep(policy.interval).await;
                }
                Attempt::Exhausted => break,
            }
        }

        let waited_ms = policy.budget().as_millis();
        let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
        tracing::warn!(
            attempts = policy.attempts,
            waited_ms,
            error_code,
            "migrate_lock_exhausted"
        );
        Err(Error::new(ErrorKind::MigrationLockUnavailable {
            waited_ms,
        }))
    }

    /// The locked session, for the work that must happen under the lock.
    pub(crate) fn connection(&mut self) -> &mut PoolConnection<Postgres> {
        &mut self.connection
    }

    /// Releases the lock and returns the connection to the pool.
    ///
    /// Best-effort by design: an unlock that fails on a dropped connection is
    /// not actionable, because the session — and with it the lock — is already
    /// gone. The pool's `after_release` hook in [`crate::migrate`] is the
    /// backstop for the path where this is never reached at all.
    pub async fn release(mut self) {
        let unlocked = self
            .connection
            .execute(sqlx::query("SELECT pg_advisory_unlock($1)").bind(ADVISORY_KEY))
            .await;
        if let Err(error) = unlocked {
            let error_code = error_code::INTERNAL_DB_QUERY.as_str();
            tracing::warn!(
                error = %error,
                error_code,
                "migrate_lock_release_ignored_error"
            );
        }
    }
}

/// Whether the migration lock is currently free, without holding it.
///
/// `pg_try_advisory_xact_lock` releases when the statement's implicit
/// transaction ends, so this can run on a pooled connection without leaving a
/// lock behind — which a session-scoped acquire plus a separate unlock cannot
/// promise (`pool_migration_lock.zig:96-104`).
///
/// # Errors
/// Returns a query error when the probe statement fails.
pub async fn probe_available(connection: &mut PoolConnection<Postgres>) -> Result<bool> {
    sqlx::query("SELECT pg_try_advisory_xact_lock($1)")
        .bind(ADVISORY_KEY)
        .fetch_one(&mut **connection)
        .await
        .map_err(|source| query(OP_PROBE, source))?
        .try_get(0)
        .map_err(|source| query(OP_PROBE, source))
}
