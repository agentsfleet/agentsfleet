//! The advisory lock that makes two migrators safe, and the bound on waiting.
//!
//! # Why the wait is bounded
//!
//! `pg_advisory_lock` waits forever. A migrator that crashed while holding the
//! lock leaves the next deploy hanging until the deploy machine's own timeout
//! fires — minutes later, with no output saying why. So this polls
//! `pg_try_advisory_lock` a bounded number of times and then fails loudly with
//! a named error, which puts "lock held" in front of an operator in seconds.
//! The bound IS the stop path: [`RetryPolicy`] is the only way out of a held lock.
//! The polls are a constant `backon` schedule with no jitter, so the bound an
//! operator reads is the bound that runs.
//!
//! # Why a query error is not a retry
//!
//! A dropped connection is not contention, and polling does not fix it. It
//! propagates immediately rather than burning the whole bound first and
//! reporting the wrong cause thirty seconds late.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use backon::{ConstantBuilder, RetryableWithContext as _};
use sqlx::pool::PoolConnection;
use sqlx::{Executor as _, Postgres};

use afd_core::error_code;

use crate::error::{Error, ErrorKind, Result, query};

/// The operations a failure names, one spelling each (RULE UFS).
const OP_ACQUIRE: &str = "migrate.acquire_lock";

/// Takes the migrate lock without waiting, answering whether it was free.
const TRY_LOCK: &str = include_str!("sql/try_lock.sql");
/// Releases the migrate lock this session holds.
const UNLOCK: &str = include_str!("sql/unlock.sql");
/// Asks whether the lock is free without keeping it past the transaction.
const PROBE_LOCK: &str = include_str!("sql/probe_lock.sql");
const OP_PROBE: &str = "migrate.probe_lock";

/// The one key the schema migration lock is taken under, cluster-wide.
///
/// Every migrator against a database takes this same key, so two replicas
/// booting together contend with each other rather than migrate in parallel.
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

/// Why one poll did not end the wait.
///
/// The retry reads this to decide whether to poll again, which is why a query
/// failure is its own arm: a dropped connection is not contention, and only
/// contention is worth another poll.
#[derive(Debug)]
enum Unheld {
    /// Another session holds the lock.
    Contended,
    /// The poll itself failed.
    Failed(Error),
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
        connection: PoolConnection<Postgres>,
        policy: RetryPolicy,
    ) -> Result<Self> {
        // A policy with no polls refuses without touching the database: the
        // retry below always makes its first call, which could win the lock.
        if policy.attempts == 0 {
            return Err(exhausted(policy));
        }
        let polls = AtomicU32::new(0);
        let retries = usize::try_from(policy.attempts.saturating_sub(1)).unwrap_or(usize::MAX);
        let (connection, polled) = (|mut connection: PoolConnection<Postgres>| {
            polls.fetch_add(1, Ordering::Relaxed);
            async move {
                let polled = try_lock(&mut connection).await;
                (connection, polled)
            }
        })
        .retry(
            ConstantBuilder::new()
                .with_delay(policy.interval)
                .with_max_times(retries),
        )
        .context(connection)
        .when(|unheld| matches!(unheld, Unheld::Contended))
        .notify(|_contended, _delay| {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let attempt = polls.load(Ordering::Relaxed);
            let retry_ms = policy.interval.as_millis();
            tracing::warn!(
                attempt,
                max_attempts = policy.attempts,
                retry_ms,
                event = "migrate_lock_contended"
            );
        })
        .await;

        match polled {
            Ok(()) => {
                let attempt = polls.load(Ordering::Relaxed);
                if attempt > 1 {
                    tracing::info!(attempt, event = "migrate_lock_acquired_after_contention");
                }
                Ok(Self { connection })
            }
            Err(Unheld::Contended) => Err(exhausted(policy)),
            Err(Unheld::Failed(error)) => Err(error),
        }
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
            .execute(sqlx::query(UNLOCK).bind(ADVISORY_KEY))
            .await;
        if let Err(error) = unlocked {
            let error_code = error_code::INTERNAL_DB_QUERY.as_str();
            tracing::warn!(
                error = %error,
                error_code,
                event = "migrate_lock_release_ignored_error"
            );
        }
    }
}

/// One poll for the lock on `connection`.
async fn try_lock(connection: &mut PoolConnection<Postgres>) -> Result<(), Unheld> {
    let acquired: bool = sqlx::query_scalar(TRY_LOCK)
        .bind(ADVISORY_KEY)
        .fetch_one(&mut **connection)
        .await
        .map_err(|source| Unheld::Failed(query(OP_ACQUIRE, source)))?;
    if acquired {
        Ok(())
    } else {
        Err(Unheld::Contended)
    }
}

/// The refusal a spent bound answers, logged once where it is raised.
fn exhausted(policy: RetryPolicy) -> Error {
    let waited_ms = policy.budget().as_millis();
    let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
    tracing::warn!(
        attempts = policy.attempts,
        waited_ms,
        error_code,
        event = "migrate_lock_exhausted"
    );
    Error::new(ErrorKind::MigrationLockUnavailable {
        waited: policy.budget(),
    })
}

/// Whether the migration lock is currently free, without holding it.
///
/// `pg_try_advisory_xact_lock` releases when the statement's implicit
/// transaction ends, so this can run on a pooled connection without leaving a
/// lock behind — which a session-scoped acquire plus a separate unlock cannot
/// promise.
///
/// # Errors
/// Returns a query error when the probe statement fails.
pub async fn probe_available(connection: &mut PoolConnection<Postgres>) -> Result<bool> {
    sqlx::query_scalar(PROBE_LOCK)
        .bind(ADVISORY_KEY)
        .fetch_one(&mut **connection)
        .await
        .map_err(|source| query(OP_PROBE, source))
}
