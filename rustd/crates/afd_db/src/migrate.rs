//! Applying the canonical migrations, once each, in order, under one lock.
//!
//! The shape is the Zig migrator's because the ledger is shared: take the
//! advisory lock, create the bookkeeping tables under it, reap versions that
//! left the canonical list, read what is already applied, then apply the rest
//! one transaction at a time. What differs is where the guarantees live —
//! [`MigrationLock`] owns the session that holds the lock, [`Migration`]
//! derives its version from its filename, and a statement list that might be a
//! truncated string literal cannot be constructed at all.

pub mod ledger;
pub mod lock;

use std::collections::BTreeSet;

use afd_core::error_code;
use sqlx::pool::PoolConnection;
use sqlx::{Acquire as _, Executor as _, Postgres};

pub use self::ledger::{FailureRow, Ledger};
pub use self::lock::{Attempt, MigrationLock, RetryPolicy};

use crate::error::{Error, ErrorKind, Result};
use crate::migration::{MIGRATIONS, Migration};
use crate::pool::Db;

/// What a run did, as data the caller can assert on or log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Versions this run applied, in application order.
    pub applied: Vec<i32>,
    /// Versions already recorded as applied, so left alone.
    pub skipped: Vec<i32>,
    /// Bookkeeping rows deleted because their version left the canonical list.
    pub reaped: u64,
}

/// What to do about a ledger version this binary does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AheadPolicy {
    /// Delete it. The pre-v2.0 teardown left rows for slots that no longer
    /// exist, and a migrate run is where they go.
    Reap,
    /// Refuse the run. A version this binary has never heard of means an older
    /// binary is looking at a database a newer one already migrated, and
    /// applying anything to it is how a schema gets torn in half.
    Refuse,
}

/// A configured migration run.
#[derive(Debug, Clone, Copy)]
pub struct Migrator {
    migrations: &'static [Migration],
    policy: RetryPolicy,
    ahead: AheadPolicy,
}

impl Default for Migrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Migrator {
    /// The canonical list, the production lock bound, reaping orphans.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: MIGRATIONS,
            policy: RetryPolicy::PRODUCTION,
            ahead: AheadPolicy::Reap,
        }
    }

    /// Runs a different list — the failure-bookkeeping proof needs a migration
    /// that fails, and there is no such file in `schema/`.
    #[must_use]
    pub const fn with_migrations(mut self, migrations: &'static [Migration]) -> Self {
        self.migrations = migrations;
        self
    }

    /// Runs under a different lock bound, so a contention test fails fast
    /// instead of waiting out the production thirty seconds.
    #[must_use]
    pub const fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Refuses a ledger written by a newer binary rather than reaping it.
    #[must_use]
    pub const fn refusing_newer(mut self) -> Self {
        self.ahead = AheadPolicy::Refuse;
        self
    }

    /// The versions this migrator knows.
    #[must_use]
    pub fn canonical_versions(&self) -> Vec<i32> {
        self.migrations.iter().map(Migration::version).collect()
    }

    /// Applies every migration not already recorded as applied.
    ///
    /// # Errors
    /// Returns a migration-refused error when the lock stays held or the ledger
    /// is ahead, a migration-failed error naming the version that would not
    /// apply, and a query error for a bookkeeping statement that failed.
    pub async fn run(&self, db: &Db) -> Result<Applied> {
        let connection = db.acquire().await?;
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let expected_versions = self.migrations.len();
        tracing::info!(expected_versions, "migrate_conn_acquired");

        let mut guard = MigrationLock::acquire(connection, self.policy).await?;
        tracing::info!("migrate_lock_acquired");

        // The lock is released on every path out of the body, including the
        // error ones. `defer` in Zig; here the body is a separate future whose
        // result is held until after the release — a `Drop` impl cannot do it,
        // because releasing is asynchronous and `Drop` cannot await.
        let outcome = self.apply_all(guard.connection()).await;
        guard.release().await;
        outcome
    }

    async fn apply_all(&self, connection: &mut PoolConnection<Postgres>) -> Result<Applied> {
        ledger::ensure_tables(connection).await?;

        let canonical: Vec<i32> = self.canonical_versions();
        if self.ahead == AheadPolicy::Refuse {
            let known: BTreeSet<i32> = canonical.iter().copied().collect();
            if let Some(found) = Ledger::read(connection).await?.version_ahead_of(&known) {
                return Err(Error::new(ErrorKind::MigrationSchemaAhead { found }));
            }
        }

        let reaped = ledger::reap_orphans(connection, &canonical).await?;
        let applied_already = Ledger::read(connection).await?.applied;

        let mut outcome = Applied {
            reaped,
            ..Applied::default()
        };
        for migration in self.migrations {
            if applied_already.contains(&migration.version()) {
                // An applied version with a stale failure row is a database
                // that recovered; clearing it here is what stops the old
                // failure being reported forever.
                ledger::clear_failure(connection, migration.version()).await;
                outcome.skipped.push(migration.version());
                continue;
            }
            apply_one(connection, migration).await?;
            outcome.applied.push(migration.version());
        }
        Ok(outcome)
    }
}

/// Applies one migration in its own transaction, recording what happened.
///
/// The transaction is the unit that matters: the schema change and the row
/// saying it happened commit together, so a crash between them is not a state
/// the ledger can be in.
async fn apply_one(connection: &mut PoolConnection<Postgres>, migration: &Migration) -> Result<()> {
    let version = migration.version();
    tracing::info!(version, name = migration.name(), "migration_start");

    let statements = match migration.statements() {
        Ok(statements) => statements,
        Err(source) => {
            // A malformed migration is refused before any of it applies —
            // splitting on a boundary inside an unterminated literal is how
            // half a statement reaches Postgres.
            let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
            tracing::error!(
                version,
                error = %source,
                error_code,
                "migrate_sql_invalid"
            );
            ledger::record_failure(connection, version, &source.to_string()).await;
            return Err(Error::new(ErrorKind::MigrationSql { version, source }));
        }
    };

    let mut count = 0_usize;
    let failure = {
        let mut transaction = match connection.begin().await {
            Ok(transaction) => transaction,
            Err(source) => return Err(crate::error::query("migrate.begin_tx", source)),
        };

        let mut failure = None;
        for statement in statements {
            if let Err(source) = transaction.execute(statement).await {
                failure = Some(source);
                break;
            }
            count += 1;
        }

        if failure.is_none()
            && let Err(error) = ledger::record_applied(&mut *transaction, version).await
        {
            // The bookkeeping insert failed rather than the migration. The
            // transaction rolls back on drop, so nothing applied, and the
            // caller gets the query error naming the insert.
            drop(transaction);
            ledger::record_failure(connection, version, &error.to_string()).await;
            return Err(error);
        }

        match failure {
            Some(source) => Some(source),
            None => transaction.commit().await.err(),
        }
    };

    if let Some(source) = failure {
        let text = source.to_string();
        ledger::record_failure(connection, version, &text).await;
        return Err(Error::new(ErrorKind::MigrationFailed { version, source }));
    }

    ledger::clear_failure(connection, version).await;
    tracing::info!(version, statements = count, "migration_applied");
    Ok(())
}
