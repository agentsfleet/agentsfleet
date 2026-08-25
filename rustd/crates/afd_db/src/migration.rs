//! The canonical migration list, and what one migration is.
//!
//! # The version IS the slot number
//!
//! `550_connector_installs.sql` applies as version 550. `schema/embed.zig`
//! states that rule and then restates each number by hand beside each file;
//! here [`version_of`] derives it from the filename during constant
//! evaluation, so a version that disagrees with the file it names is not a
//! mistake anyone can make. A filename that is not `<digits>_<name>.sql` fails
//! the build.
//!
//! # Why this list is written out rather than globbed
//!
//! `include_str!` needs a literal path, so the alternative is a build script
//! that scans `schema/`. A scan cannot tell a file that belongs in the ledger
//! from one that was dropped in the directory, and it would make the migration
//! set depend on the state of a working tree. The list is explicit, and
//! `test_migration_list_matches_schema_directory` compares it against both the
//! directory and `schema/embed.zig` — so an addition to either side that is
//! missing here fails a test rather than a production migrate.

use crate::sql::{SplitError, SqlStatements};

/// One versioned schema migration: the file, its slot number, and its SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

impl Migration {
    /// The slot number this migration applies as.
    #[must_use]
    pub const fn version(&self) -> i32 {
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

    /// Builds a migration that is not one of the committed schema files.
    ///
    /// The failure-bookkeeping proof needs a migration that fails, and
    /// `schema/` contains no such file — nor should it. Behind `test-util` so
    /// the production binary has no way to apply SQL that is not committed.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn for_test(version: i32, name: &'static str, sql: &'static str) -> Self {
        Self { version, name, sql }
    }

    /// The statements this migration applies, in order.
    ///
    /// # Errors
    /// Returns [`SplitError`] when the file ends inside a string, quoted
    /// identifier, dollar-quoted body, or block comment.
    pub fn statements(&self) -> Result<SqlStatements<'static>, SplitError> {
        SqlStatements::new(self.sql)
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
pub fn version_from_name(name: &str) -> i32 {
    version_of(name)
}

/// Every migration this binary knows, in application order.
///
/// Order is ascending version, which is also dependency order: `1xx` substrate
/// before `2xx` identity before `5xx` fleets, because that is the order a
/// database bootstrapped from empty must create them in.
pub static MIGRATIONS: &[Migration] = &[
    // 1xx substrate — runs before any table exists
    migration!("100_schemas.sql"),
    migration!("110_roles_and_privileges.sql"),
    // 2xx identity — the tenant root and everything that authenticates
    migration!("200_tenants.sql"),
    migration!("210_workspaces.sql"),
    migration!("220_users.sql"),
    migration!("230_memberships.sql"),
    migration!("240_api_keys.sql"),
    migration!("250_cli_credentials.sql"),
    // 3xx secrets
    migration!("300_vault_secrets.sql"),
    // 4xx catalogue
    migration!("400_model_library.sql"),
    migration!("410_model_catalogue_revision.sql"),
    migration!("420_platform_provider_defaults.sql"),
    migration!("430_tenant_model_selection.sql"),
    migration!("440_tenant_model_entries.sql"),
    migration!("450_fleet_library.sql"),
    migration!("460_tenant_fleet_library.sql"),
    // 5xx fleets
    migration!("500_fleets.sql"),
    migration!("510_fleet_sessions.sql"),
    migration!("520_fleet_schedules.sql"),
    migration!("540_integration_grants.sql"),
    migration!("550_connector_installs.sql"),
    migration!("551_connector_installs_delete_privilege.sql"),
    migration!("560_connector_channels.sql"),
    migration!("570_user_preferences.sql"),
    // 6xx runner control plane
    migration!("600_runners.sql"),
    migration!("610_runner_leases.sql"),
    migration!("620_runner_lease_indexes.sql"),
    migration!("630_runner_affinity.sql"),
    migration!("640_runner_events.sql"),
    migration!("650_runner_lifetime_counters.sql"),
    migration!("660_runner_selftest_columns.sql"),
    migration!("670_runner_extra_binds.sql"),
    // 7xx money
    migration!("700_tenant_wallet.sql"),
    migration!("710_usage_ledger.sql"),
    migration!("720_usage_ledger_indexes.sql"),
    // 8xx history
    migration!("800_fleet_events.sql"),
    migration!("810_fleet_approval_gates.sql"),
    migration!("811_fleet_approval_gates_event_binding.sql"),
    migration!("820_memory_entries.sql"),
    migration!("830_repair_pr_links.sql"),
    migration!("831_repair_run_results.sql"),
    migration!("832_repair_pr_merge_correlation.sql"),
    migration!("833_fleet_approval_gates_spend.sql"),
    migration!("834_repair_production_results.sql"),
    migration!("835_repair_verifications.sql"),
    migration!("880_fleet_activity_counters.sql"),
    migration!("890_fleet_activity_counter_triggers.sql"),
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
const fn version_of(name: &str) -> i32 {
    let mut rest = name.as_bytes();
    let mut version: i32 = 0;
    let mut digits: usize = 0;

    while let [digit @ b'0'..=b'9', tail @ ..] = rest {
        version = version * 10 + (*digit - b'0') as i32;
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
