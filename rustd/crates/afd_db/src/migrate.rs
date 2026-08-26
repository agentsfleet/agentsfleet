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
///
/// Neither policy will delete an unknown version AT OR ABOVE the canonical
/// floor — that refusal is unconditional, and is not what this chooses
/// between. See [`Migrator::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AheadPolicy {
    /// Delete a RETIRED slot — a version below the canonical floor, left by
    /// the numbering that preceded this one. The pre-v2.0 teardown left rows
    /// for slots that no longer exist, and a migrate run is where they go.
    ReapRetired,
    /// Refuse the run over any version this binary does not know, retired slots
    /// included. Stricter than the unconditional floor check, and the setting
    /// a test uses when it wants an unknown version to be an error rather than
    /// a tidy-up.
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
    /// The canonical list, the production lock bound, reaping retired slots.
    ///
    /// Retired slots only. An unknown version at or above the canonical floor
    /// refuses the run whatever this is set to — see [`Self::run`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: MIGRATIONS,
            policy: RetryPolicy::PRODUCTION,
            ahead: AheadPolicy::ReapRetired,
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

    /// Refuses ANY version this binary does not know, retired slots included.
    ///
    /// A ledger written by a newer binary is refused without this — the floor
    /// check below is unconditional. What this adds is refusing a retired slot
    /// below the floor too, instead of reaping it.
    #[must_use]
    pub const fn refusing_newer(mut self) -> Self {
        self.ahead = AheadPolicy::Refuse;
        self
    }

    /// The highest version this migrator knows, or `None` if it knows none.
    ///
    /// The line between "a slot that was retired" and "a schema from the
    /// future" — kept because a test naming a GAP needs the top of the range to
    /// search within, not because the refusal consults it. A migrator handed an
    /// empty list has no ceiling to draw.
    #[must_use]
    pub fn ceiling(&self) -> Option<i32> {
        self.migrations.iter().map(Migration::version).max()
    }

    /// The lowest version this migrator knows, or `None` if it knows none.
    ///
    /// The line between "a slot from the previous numbering" and "a slot this
    /// scheme owns". Reaping happens strictly below it; an unknown version at
    /// or above it refuses the run.
    #[must_use]
    pub fn floor(&self) -> Option<i32> {
        self.migrations.iter().map(Migration::version).min()
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
        let before = Ledger::read(connection).await?;

        // UNCONDITIONAL, and it runs BEFORE the reap on purpose.
        //
        // An unknown version AT OR ABOVE THE FLOOR was written by a binary that
        // has a migration in this scheme's range and this one does not — so
        // something NEWER wrote it. Reaping it would delete that deployment's
        // migration history and then apply this binary's schema on top of one
        // it does not understand — a schema torn between two versions, with a
        // ledger that no longer records how it got there. That is not a
        // tidy-up, so it is not something a policy gets to opt into: an older
        // image rolled onto an upgraded database stops here, having changed
        // nothing.
        //
        // The FLOOR, not the ceiling. A ceiling test only catches the unknown
        // version that happens to be the highest, and the numbering leaves gaps
        // ON PURPOSE — 550, 551, 560. A newer release filling one lands BELOW
        // an older binary's ceiling, read as a retired slot, and had its record
        // reaped while its schema change stayed applied.
        //
        // The reap below still clears slots beneath the floor, which is the
        // case the pre-v2.0 teardown actually left behind (`001_*`…`005_*`,
        // deleted when the scheme renumbered to 100+) and the one reaping is
        // for.
        let known: BTreeSet<i32> = canonical.iter().copied().collect();
        if let Some(floor) = self.floor()
            && let Some(found) = before.version_unknown_at_or_above(floor, &known)
        {
            let error_code = error_code::STARTUP_MIGRATION_CHECK.as_str();
            tracing::error!(
                found,
                floor,
                error_code,
                event = "migrate_refused_schema_ahead"
            );
            return Err(Error::new(ErrorKind::MigrationSchemaAhead { found }));
        }

        if self.ahead == AheadPolicy::Refuse
            && let Some(found) = before.version_ahead_of(&known)
        {
            return Err(Error::new(ErrorKind::MigrationSchemaAhead { found }));
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
    tracing::info!(version, name = migration.name(), event = "migration_start");

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
                event = "migrate_sql_invalid"
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
    tracing::info!(version, statements = count, event = "migration_applied");
    Ok(())
}
