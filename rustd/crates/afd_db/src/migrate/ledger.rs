//! The bookkeeping two migrators share: what applied, and what failed.
//!
//! Both tables are created here with `IF NOT EXISTS` and the same column types
//! the Zig daemon writes (`pool_migrations.zig:38-57`), because the same
//! database is migrated by whichever binary a deploy happens to run. A column
//! that differed by width or nullability would show up as a constraint
//! violation on the first row the other binary wrote.
//!
//! `applied_at` and `failed_at` are `BIGINT` milliseconds since the epoch, not
//! `timestamptz`. That is the Zig daemon's choice and it is now a data format,
//! so it stays.

use std::collections::{BTreeMap, BTreeSet};

use afd_core::error_code;
use sqlx::pool::PoolConnection;
use sqlx::{Executor as _, Postgres, Row as _};

use crate::error::{Result, query};

/// The operations a failure names. One spelling each (RULE UFS): these strings
/// are what an operator greps for, so two spellings of one step is two searches.
const OP_LOAD_APPLIED: &str = "migrate.load_applied_versions";
const OP_LOAD_FAILURES: &str = "migrate.load_failures";
const OP_REAP: &str = "migrate.reap_orphans";

const CREATE_AUDIT_SCHEMA: &str = "CREATE SCHEMA IF NOT EXISTS audit";

const CREATE_SCHEMA_MIGRATIONS: &str = "CREATE TABLE IF NOT EXISTS audit.schema_migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  BIGINT NOT NULL
)";

const CREATE_SCHEMA_MIGRATION_FAILURES: &str =
    "CREATE TABLE IF NOT EXISTS audit.schema_migration_failures (
    version     INTEGER PRIMARY KEY,
    failed_at   BIGINT NOT NULL,
    error_text  TEXT NOT NULL
)";

/// One recorded failure, as the ledger holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureRow {
    /// Milliseconds since the epoch, as written.
    pub failed_at: i64,
    /// Why it failed, in the form an operator reads.
    pub error_text: String,
}

/// A read of both bookkeeping tables.
///
/// Returned as data rather than logged, so `/readyz`, the migrate subcommand,
/// and the parity tests all read the same thing instead of three call sites
/// each writing their own query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Versions recorded as applied.
    pub applied: BTreeSet<i32>,
    /// Versions with a failure row, and what it says.
    pub failures: BTreeMap<i32, FailureRow>,
}

impl Ledger {
    /// Reads both tables.
    ///
    /// # Errors
    /// Returns a query error when either read fails — including the case where
    /// the tables do not exist yet, which is a caller ordering mistake rather
    /// than something to paper over with a default.
    pub async fn read(connection: &mut PoolConnection<Postgres>) -> Result<Self> {
        let applied = sqlx::query("SELECT version FROM audit.schema_migrations")
            .fetch_all(&mut **connection)
            .await
            .map_err(|source| query(OP_LOAD_APPLIED, source))?
            .iter()
            .map(|row| row.try_get::<i32, _>(0))
            .collect::<Result<BTreeSet<i32>, _>>()
            .map_err(|source| query(OP_LOAD_APPLIED, source))?;

        let mut failures = BTreeMap::new();
        let rows = sqlx::query(
            "SELECT version, failed_at, error_text FROM audit.schema_migration_failures",
        )
        .fetch_all(&mut **connection)
        .await
        .map_err(|source| query(OP_LOAD_FAILURES, source))?;
        for row in &rows {
            let version: i32 = row
                .try_get(0)
                .map_err(|source| query(OP_LOAD_FAILURES, source))?;
            let failed_at: i64 = row
                .try_get(1)
                .map_err(|source| query(OP_LOAD_FAILURES, source))?;
            let error_text: String = row
                .try_get(2)
                .map_err(|source| query(OP_LOAD_FAILURES, source))?;
            failures.insert(
                version,
                FailureRow {
                    failed_at,
                    error_text,
                },
            );
        }

        Ok(Self { applied, failures })
    }

    /// The first recorded version absent from `canonical`, if any.
    ///
    /// Both tables are checked: a failure row from a version this binary does
    /// not know says the same thing an applied row does — the database was
    /// migrated by something newer.
    ///
    /// Note what this does NOT distinguish: a slot the canonical list RETIRED
    /// reads as "absent" exactly like a version from the future does.
    /// [`Self::version_unknown_at_or_above`] is the one that tells them apart,
    /// and the one a reaping run must consult.
    #[must_use]
    pub fn version_ahead_of(&self, canonical: &BTreeSet<i32>) -> Option<i32> {
        self.applied
            .iter()
            .chain(self.failures.keys())
            .find(|version| !canonical.contains(version))
            .copied()
    }

    /// The first recorded version at or above `floor` that `canonical` does not
    /// contain, if any.
    ///
    /// The discriminator between the two kinds of unknown version, and the
    /// reason it is a separate question from [`Self::version_ahead_of`]. The
    /// line is the FLOOR — the lowest version this binary knows — not the
    /// ceiling:
    ///
    /// - **Below the floor** — a slot from the numbering that preceded this
    ///   one. The pre-v2.0 teardown deleted `001_*`…`005_*` when the scheme
    ///   renumbered to `100`+, and left their rows behind; a migrate run is
    ///   where those go. Reaping is correct.
    /// - **At or above the floor and absent** — a slot inside the range THIS
    ///   scheme owns, which this binary does not have. Something newer wrote
    ///   it. Reaping it deletes another binary's migration history and then
    ///   applies this binary's schema on top of one it does not understand.
    ///
    /// A ceiling test would only catch the second case when the unknown version
    /// happens to be the highest. The numbering leaves gaps ON PURPOSE — `550`,
    /// `551`, `560` — so a newer release filling one lands BELOW an older
    /// binary's ceiling and read as a retired slot: the run reaped a live
    /// migration's record and carried on. That is the case this asks about.
    ///
    /// Both tables are checked, for the reason above.
    #[must_use]
    pub fn version_unknown_at_or_above(
        &self,
        floor: i32,
        canonical: &BTreeSet<i32>,
    ) -> Option<i32> {
        self.applied
            .iter()
            .chain(self.failures.keys())
            .find(|version| **version >= floor && !canonical.contains(version))
            .copied()
    }
}

/// Creates the audit schema and both bookkeeping tables.
///
/// Runs under the migration lock, never before it: `CREATE TABLE IF NOT
/// EXISTS` is not race-safe, so two fresh-database boots that skipped the lock
/// for "just the DDL" can still collide (`pool_migrations.zig:160-163`).
///
/// # Errors
/// Returns a query error when any of the three statements fails.
pub async fn ensure_tables(connection: &mut PoolConnection<Postgres>) -> Result<()> {
    for statement in [
        CREATE_AUDIT_SCHEMA,
        CREATE_SCHEMA_MIGRATIONS,
        CREATE_SCHEMA_MIGRATION_FAILURES,
    ] {
        connection
            .execute(statement)
            .await
            .map_err(|source| query("migrate.ensure_tables", source))?;
    }
    Ok(())
}

/// Deletes bookkeeping rows for slots that predate this numbering scheme.
///
/// # Why the floor is in the statement and not only in the caller
///
/// Deleting every row absent from the canonical list destroys another
/// deployment's migration history: a version a NEWER binary wrote is absent
/// from an older binary's list in exactly the way a retired slot is, and the
/// number alone cannot tell them apart. The FLOOR can. Everything below the
/// lowest version this binary knows belongs to the numbering that preceded it
/// — the `001_*`…`005_*` files the pre-v2.0 teardown deleted when the scheme
/// renumbered to `100`+ — and nothing a newer release adds lands there, because
/// a newer release extends the scheme upward and into its reserved gaps.
///
/// [`crate::migrate::Migrator`] refuses the run over any unknown version at or
/// above the floor before calling this; `version < $1` is that same invariant
/// expressed where the deletion happens, so a future caller that forgets the
/// check cannot destroy history with it. A caller with no canonical versions at
/// all has no floor to apply and reaps nothing — an empty migration list is not
/// a licence to empty the ledger.
///
/// # Errors
/// Returns a query error when either delete fails.
pub async fn reap_orphans(
    connection: &mut PoolConnection<Postgres>,
    canonical: &[i32],
) -> Result<u64> {
    // An empty migration list is not a licence to empty the ledger.
    let Some(floor) = canonical.iter().copied().min() else {
        return Ok(0);
    };

    let reaped = sqlx::query("DELETE FROM audit.schema_migrations WHERE version < $1")
        .bind(floor)
        .execute(&mut **connection)
        .await
        .map_err(|source| query(OP_REAP, source))?
        .rows_affected();

    sqlx::query("DELETE FROM audit.schema_migration_failures WHERE version < $1")
        .bind(floor)
        .execute(&mut **connection)
        .await
        .map_err(|source| query(OP_REAP, source))?;

    if reaped > 0 {
        tracing::info!(reaped, scope = "orphan_rows", "migration_reap");
    }
    Ok(reaped)
}

/// Records a version as applied, inside the caller's transaction.
///
/// # Errors
/// Returns a query error when the insert fails, which rolls the caller's
/// transaction back — the row and the schema change commit together or not at
/// all.
pub async fn record_applied<'e, E>(executor: E, version: i32) -> Result<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2)")
        .bind(version)
        .bind(now_millis())
        .execute(executor)
        .await
        .map_err(|source| query("migrate.insert_schema_migrations", source))?;
    Ok(())
}

/// Upserts a failure row. Best-effort: the migration has already failed, and a
/// bookkeeping write that also fails must not replace the real cause.
pub async fn record_failure(
    connection: &mut PoolConnection<Postgres>,
    version: i32,
    error_text: &str,
) {
    let written = sqlx::query(
        "INSERT INTO audit.schema_migration_failures (version, failed_at, error_text)
         VALUES ($1, $2, $3)
         ON CONFLICT (version) DO UPDATE
         SET failed_at = EXCLUDED.failed_at,
             error_text = EXCLUDED.error_text",
    )
    .bind(version)
    .bind(now_millis())
    .bind(error_text)
    .execute(&mut **connection)
    .await;

    if let Err(error) = written {
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let error_code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::warn!(
            version,
            error = %error,
            error_code,
            "migrate_failure_row_ignored_error"
        );
    }
}

/// Clears a version's failure row once it applies — or once it is found already
/// applied, which is how a database that recovered stops reporting an old
/// failure forever.
pub async fn clear_failure(connection: &mut PoolConnection<Postgres>, version: i32) {
    let cleared = sqlx::query("DELETE FROM audit.schema_migration_failures WHERE version = $1")
        .bind(version)
        .execute(&mut **connection)
        .await;

    if let Err(error) = cleared {
        let error_code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::warn!(
            version,
            error = %error,
            error_code,
            "migrate_failure_clear_ignored_error"
        );
    }
}

/// Milliseconds since the epoch, from the workspace clock.
///
/// A private copy of this used to map a pre-epoch clock to `0`. The Zig
/// daemon's `clock.zig` returns the NEGATIVE reading for the same host and says
/// why in its own words — a silent epoch-0 return corrupts `UUIDv7` ordering —
/// so the two binaries answered a broken host differently while writing to the
/// same `audit.schema_migrations` table. [`afd_core::clock::now`] is the single
/// reading both the ledger and everything after it share.
fn now_millis() -> i64 {
    afd_core::clock::now().as_millis()
}
