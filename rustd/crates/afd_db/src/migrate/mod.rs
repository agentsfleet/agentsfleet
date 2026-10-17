//! Applying the canonical migrations, once each, in order, under one lock.
//!
//! sqlx's `Migrator` does the applying: one transaction per slot, a checksum
//! per slot in `audit._sqlx_migrations`, and a refusal when an applied slot's
//! file changed or the ledger records a slot this binary does not know. The
//! lock is ours. sqlx's own lock waits forever, so it is switched off and
//! [`MigrationLock`] polls a bounded number of times instead, which puts "lock
//! held" in front of an operator in seconds rather than hanging a deploy.
//!
//! Every database this binary migrates started empty under it, so there is no
//! older ledger to read or refuse.

pub mod lock;

use std::collections::BTreeSet;

use afd_core::error_code;
use sqlx::Postgres;
use sqlx::Row as _;
use sqlx::migrate::MigrateError;
use sqlx::pool::PoolConnection;

pub use self::lock::{MigrationLock, RetryPolicy};

use crate::error::{Error, ErrorKind, Result, query};
use crate::migration::{MIGRATIONS, Migration};
use crate::pool::Db;

/// Where sqlx keeps its ledger. `audit` is created by the migrator before the
/// ledger, because no slot has run yet when the ledger is first written.
const LEDGER_SCHEMA: &str = "audit";
const LEDGER_TABLE: &str = "audit._sqlx_migrations";

/// The operations a ledger failure names (RULE UFS).
const OP_READ_LEDGER: &str = "migrate.read_ledger";
const OP_WRITE_LEDGER: &str = "migrate.write_ledger";

/// Whether the ledger exists yet; `to_regclass` answers NULL for a missing table
/// where a read would fail. A fresh database has none until the first run.
const LEDGER_EXISTS: &str = include_str!("sql/ledger_exists.sql");
const READ_LEDGER: &str = include_str!("sql/read_ledger.sql");

/// What a run did, as data the caller can assert on or log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Versions this run applied, in application order.
    pub applied: Vec<i64>,
    /// Versions already recorded as applied, so left alone.
    pub skipped: Vec<i64>,
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

    /// Runs a different list — the failed-slot and edited-slot proofs need
    /// migrations that are not in `schema/`.
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

    /// The versions this migrator knows.
    #[must_use]
    pub fn canonical_versions(&self) -> Vec<i64> {
        self.migrations.iter().map(Migration::version).collect()
    }

    /// Applies every migration not already recorded as applied.
    ///
    /// # Errors
    /// Returns a migration-refused error when the lock stays held, an applied
    /// slot was edited, or the ledger records a slot this binary does not know;
    /// a migration-failed error naming the slot that would not apply; and a
    /// query error for a ledger statement that failed.
    pub async fn run(&self, db: &Db) -> Result<Applied> {
        let connection = db.acquire().await?;
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let expected_versions = self.migrations.len();
        tracing::info!(expected_versions, event = "migrate_conn_acquired");

        let mut guard = MigrationLock::acquire(connection, self.policy).await?;
        tracing::info!(event = "migrate_lock_acquired");

        // Released on every path out, including the error ones. The body is a
        // separate future whose result is held until after the release, because
        // releasing is asynchronous and `Drop` cannot await.
        let outcome = self.apply_all(guard.connection()).await;
        guard.release().await;
        outcome
    }

    async fn apply_all(&self, connection: &mut PoolConnection<Postgres>) -> Result<Applied> {
        let before = applied_versions(connection).await?;

        let mut migrator = sqlx::migrate::Migrator::with_migrations(
            self.migrations.iter().map(Migration::to_sqlx).collect(),
        );
        migrator.set_locking(false);
        migrator.dangerous_set_table_name(LEDGER_TABLE);
        migrator.create_schema(LEDGER_SCHEMA);
        migrator.run(&mut **connection).await.map_err(refusal)?;

        let after = applied_versions(connection).await?;
        let mut outcome = Applied::default();
        for version in self.canonical_versions() {
            if before.contains(&version) {
                outcome.skipped.push(version);
            } else if after.contains(&version) {
                tracing::info!(version, event = "migration_applied");
                outcome.applied.push(version);
            }
        }
        Ok(outcome)
    }
}

/// The versions sqlx recorded as applied.
async fn applied_versions(connection: &mut PoolConnection<Postgres>) -> Result<BTreeSet<i64>> {
    let exists: bool = sqlx::query_scalar(LEDGER_EXISTS)
        .fetch_one(&mut **connection)
        .await
        .map_err(|source| query(OP_READ_LEDGER, source))?;
    if !exists {
        return Ok(BTreeSet::new());
    }
    sqlx::query(READ_LEDGER)
        .fetch_all(&mut **connection)
        .await
        .map_err(|source| query(OP_READ_LEDGER, source))?
        .iter()
        .map(|row| row.try_get::<i64, _>(0))
        .collect::<Result<BTreeSet<i64>, _>>()
        .map_err(|source| query(OP_READ_LEDGER, source))
}

/// Names what sqlx refused, logged once where it is raised.
fn refusal(source: MigrateError) -> Error {
    let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
    let kind = match source {
        MigrateError::ExecuteMigration(source, version) => {
            tracing::error!(version, error_code, event = "migration_failed");
            ErrorKind::MigrationFailed { version, source }
        }
        // A ledger statement failed rather than a slot: the slot's transaction
        // rolled back with it, so this is a query error, not a failed slot.
        MigrateError::Execute(source) => {
            tracing::error!(error = %source, error_code, event = "migrate_ledger_failed");
            return query(OP_WRITE_LEDGER, source);
        }
        MigrateError::VersionMismatch(version) => {
            tracing::error!(version, error_code, event = "migrate_refused_slot_edited");
            ErrorKind::MigrationEdited { version }
        }
        MigrateError::VersionMissing(found) => {
            tracing::error!(found, error_code, event = "migrate_refused_schema_ahead");
            ErrorKind::MigrationSchemaAhead { found }
        }
        other => {
            tracing::error!(error = %other, error_code, event = "migrate_refused");
            ErrorKind::MigrationRefused { source: other }
        }
    };
    Error::new(kind)
}
