//! The canonical migration list, and what one migration is.
//!
//! # The version IS the slot number
//!
//! `1610_connector_installs.sql` applies as version 1610. [`version_of`] derives
//! the number from the filename during constant evaluation, so a version that
//! disagrees with the file it names is not a mistake anyone can make. A
//! filename that is not `<digits>_<name>.sql` fails the build. Slot files are
//! named `NNNN_<schema>_<table>.sql`.
//!
//! # sqlx applies them
//!
//! [`Migration::to_sqlx`] hands each file to sqlx's `Migrator`, which records it
//! with a checksum in `audit._sqlx_migrations` and refuses an applied slot whose
//! file later changed. The list itself stays ours, for the reason below.
//!
//! # Why this list is written out rather than globbed
//!
//! `include_str!` needs a literal path, so the alternative is a build script
//! that scans `schema/`. A scan cannot tell a file that belongs in the ledger
//! from one that was dropped in the directory, and it would make the migration
//! set depend on the state of a working tree. The list is explicit, and
//! `scripts/check_rust_source.py` (rule `rust-migration-list`, in
//! `make lint-all`) compares it against the tracked `schema/*.sql` files — so
//! a file added to `schema/` and missing here fails the lint rather than a
//! production migrate.

use std::borrow::Cow;

use sqlx::SqlSafeStr as _;

/// One versioned schema migration: the file, its slot number, and its SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

impl Migration {
    /// The slot number this migration applies as.
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// The schema filename, which is also where the version comes from.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The whole file, unsplit.
    #[must_use]
    pub const fn sql(&self) -> &'static str {
        self.sql
    }

    /// This file as sqlx applies it: one transaction, checksummed.
    ///
    /// The SQL is a committed schema file embedded with `include_str!`, never
    /// text assembled at run time, which is what `AssertSqlSafe` asserts.
    #[must_use]
    pub fn to_sqlx(&self) -> sqlx::migrate::Migration {
        sqlx::migrate::Migration::new(
            self.version,
            Cow::Borrowed(self.name),
            sqlx::migrate::MigrationType::Simple,
            sqlx::AssertSqlSafe(self.sql).into_sql_str(),
            false,
        )
    }

    /// Builds a migration that is not one of the committed schema files.
    ///
    /// The failed-slot and edited-slot proofs need migrations that are not
    /// committed, and `schema/` contains no such file — nor should it. Behind `test-util` so
    /// the production binary has no way to apply SQL that is not committed.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn for_test(version: i64, name: &'static str, sql: &'static str) -> Self {
        Self { version, name, sql }
    }
}

/// Binds one schema file to the migration it becomes.
///
/// A macro rather than a function because `include_str!` takes a literal, and
/// the literal has to be built from the same token the version is derived from
/// — that is what keeps the two from drifting.
macro_rules! migration {
    ($file:literal) => {
        Migration {
            version: version_of($file),
            name: $file,
            sql: include_str!(concat!("../../../../schema/", $file)),
        }
    };
}

/// Derives a slot number the way the `MIGRATIONS` list does, at runtime.
///
/// `version_of` is a `const fn`, and every production call site is a `static`
/// initialiser — so it runs during constant evaluation and its body never
/// executes at runtime. That is what a caller wants and it leaves the
/// derivation untestable AS CODE: nothing can hand it a name and check the
/// number. This is that caller. A `const fn` is an ordinary function when the
/// context is not const, so this exercises the same body the build does.
#[cfg(feature = "test-util")]
#[must_use]
pub fn version_from_name(name: &str) -> i64 {
    version_of(name)
}

/// Every migration this binary knows, in application order.
///
/// Order is ascending version, which is also foreign-key order: a table follows
/// every table it references, across schemas, and a foreign key that closes a
/// cycle (a head pointer) has its own slot after both of its tables.
pub static MIGRATIONS: &[Migration] = &[
    // 10xx substrate — runs before any table exists
    migration!("1000_schemas.sql"),
    migration!("1010_roles_and_privileges.sql"),
    // 11xx identity — the tenant root and everything that authenticates
    migration!("1100_identity_tenants.sql"),
    migration!("1110_identity_workspaces.sql"),
    migration!("1120_identity_users.sql"),
    migration!("1130_identity_memberships.sql"),
    migration!("1140_identity_invites.sql"),
    migration!("1150_identity_api_keys.sql"),
    migration!("1160_identity_cli_credentials.sql"),
    migration!("1170_identity_user_preferences.sql"),
    // 12xx vault
    migration!("1200_vault_secrets.sql"),
    // 13xx model catalogue and tenant models
    migration!("1300_model_platform_models.sql"),
    migration!("1305_model_platform_model_prices.sql"),
    migration!("1310_model_catalogue_version.sql"),
    migration!("1320_model_platform_defaults.sql"),
    migration!("1330_model_tenant_models.sql"),
    migration!("1340_model_tenant_selections.sql"),
    // 14xx authoring Library — cards and their frozen revisions
    migration!("1400_authoring_library_entries.sql"),
    migration!("1410_authoring_library_revisions.sql"),
    migration!("1420_authoring_library_entries_head_revision.sql"),
    // 15xx fleets and their frozen setup history
    migration!("1500_fleet_fleets.sql"),
    migration!("1510_fleet_revisions.sql"),
    migration!("1520_fleet_fleets_head_revision.sql"),
    // 16xx connector — grants, outside accounts, channels
    migration!("1600_connector_integration_grants.sql"),
    migration!("1610_connector_installs.sql"),
    migration!("1620_connector_channels.sql"),
    // 17xx fleet work — events, admissions, gates, obligations, schedules
    migration!("1700_fleet_events.sql"),
    migration!("1710_fleet_tool_calls.sql"),
    migration!("1720_fleet_admissions.sql"),
    migration!("1730_fleet_approval_gates.sql"),
    migration!("1740_fleet_obligations.sql"),
    migration!("1750_fleet_schedules.sql"),
    migration!("1760_fleet_sessions.sql"),
    migration!("1770_fleet_activity_counters.sql"),
    // 18xx runner control plane
    migration!("1800_runner_runners.sql"),
    migration!("1810_runner_leases.sql"),
    migration!("1820_runner_fleet_slots.sql"),
    migration!("1830_runner_events.sql"),
    migration!("1840_runner_lifetime_counters.sql"),
    // 19xx authoring chat — drafts, turns, attempts, receipts
    migration!("1900_authoring_drafts.sql"),
    migration!("1910_authoring_candidates.sql"),
    migration!("1920_authoring_drafts_head_candidate.sql"),
    migration!("1930_authoring_messages.sql"),
    migration!("1940_authoring_attempts.sql"),
    migration!("1950_authoring_dispatches.sql"),
    migration!("1960_authoring_activity_receipts.sql"),
    migration!("1970_authoring_answer_receipts.sql"),
    migration!("1980_authoring_replay_receipts.sql"),
    // 20xx billing — the wallet and every charge
    migration!("2000_billing_wallets.sql"),
    migration!("2010_billing_fleet_charges.sql"),
    migration!("2020_billing_authoring_settlements.sql"),
    migration!("2030_billing_authoring_report_receipts.sql"),
    migration!("2040_billing_usage_lines.sql"),
    migration!("2050_billing_holds.sql"),
    // 21xx memory
    migration!("2100_memory_entries.sql"),
];

/// Derives the slot number from the filename during constant evaluation.
///
/// Slice patterns rather than index arithmetic, the way
/// `afd_core::error_code::is_registry_spelling` reads every bound out of the
/// pattern instead of out of a comparison.
///
/// # Panics
/// During constant evaluation, when the name does not start with digits
/// followed by `_`. Every call site is a `static` initialiser, so a malformed
/// filename fails the build and never the process.
const fn version_of(name: &str) -> i64 {
    let mut rest = name.as_bytes();
    let mut version: i64 = 0;
    let mut digits: usize = 0;

    while let [digit @ b'0'..=b'9', tail @ ..] = rest {
        version = version * 10 + (*digit - b'0') as i64;
        digits += 1;
        rest = tail;
    }

    assert!(
        digits > 0,
        "schema filename must start with its slot number"
    );
    assert!(
        matches!(rest, [b'_', ..]),
        "schema filename must separate its slot number with an underscore"
    );
    version
}
