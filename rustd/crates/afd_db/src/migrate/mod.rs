//! Applying the canonical migrations, once each, in order, under one lock.
//!
//! The order is fixed by the ledger every deployment already has: take the
//! advisory lock, create the bookkeeping tables under it, refuse a ledger that
//! records a version this binary does not know, then apply the rest one
//! transaction at a time. The guarantees live in the types —
//! [`MigrationLock`] owns the session that holds the lock, [`Migration`]
//! derives its version from its filename, and a statement list that might be a
//! truncated string literal cannot be constructed at all.

pub mod ledger;
pub mod lock;

use std::collections::BTreeSet;

use afd_core::error_code;
use sqlx::pool::PoolConnection;
use sqlx::{Acquire as _, Postgres};

pub use self::ledger::{FailureRow, Ledger};
pub use self::lock::{MigrationLock, RetryPolicy};

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
}

/// A configured migration run.
#[derive(Debug, Clone, Copy)]
pub struct Migrator {
    migrations: &'static [Migration],
    policy: RetryPolicy,
}

impl Default for Migrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Migrator {
    /// The canonical list and the production lock bound.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: MIGRATIONS,
            policy: RetryPolicy::PRODUCTION,
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

    /// The highest version this migrator knows, or `None` if it knows none.
    ///
    /// Kept because a test naming a GAP needs the top of the range to search
    /// within, not because the refusal consults it. A migrator handed an empty
    /// list has no ceiling to draw.
    #[must_use]
    pub fn ceiling(&self) -> Option<i32> {
        self.migrations.iter().map(Migration::version).max()
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
        tracing::info!(expected_versions, event = "migrate_conn_acquired");

        let mut guard = MigrationLock::acquire(connection, self.policy).await?;
        tracing::info!(event = "migrate_lock_acquired");

        // The lock is released on every path out of the body, including the
        // error ones. The body is a separate future whose result is held until
        // after the release — a `Drop` impl cannot do it, because releasing is
        // asynchronous and `Drop` cannot await.
        let outcome = self.apply_all(guard.connection()).await;
        guard.release().await;
        outcome
    }

    async fn apply_all(&self, connection: &mut PoolConnection<Postgres>) -> Result<Applied> {
        ledger::ensure_tables(connection).await?;

        let canonical: Vec<i32> = self.canonical_versions();
        let before = Ledger::read(connection).await?;

        // A version this binary does not know was written by something newer.
        // Applying this binary's schema on top of it would leave a schema torn
        // between two versions, so an older image rolled onto an upgraded
        // database stops here, having changed nothing. Every unknown version
        // counts, including one in a reserved gap below this binary's ceiling.
        let known: BTreeSet<i32> = canonical.iter().copied().collect();
        if let Some(found) = before.version_ahead_of(&known) {
            let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
            tracing::error!(found, error_code, event = "migrate_refused_schema_ahead");
            return Err(Error::new(ErrorKind::MigrationSchemaAhead { found }));
        }

        let applied_already = Ledger::read(connection).await?.applied;

        let mut outcome = Applied::default();
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
    tracing::info!(version, name = migration.name(), event = "migration_start");

    let failure = {
        let mut transaction = match connection.begin().await {
            Ok(transaction) => transaction,
            Err(source) => return Err(crate::error::query("migrate.begin_tx", source)),
        };

        let failure = sqlx::raw_sql(migration.sql())
            .execute(&mut *transaction)
            .await
            .err();

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
    tracing::info!(version, event = "migration_applied");
    Ok(())
}
