//! The canonical migration list, and what one migration is.
//!
//! # The version IS the file's timestamp
//!
//! `20261108122200_connector_installs.sql` applies as version 20261108122200. [`version_of`]
//! derives the number from the filename during constant evaluation, so a
//! version that disagrees with the file it names is not a mistake anyone can
//! make. A filename that does not open with a UTC `yyyymmddHHMMSS` timestamp
//! and `_` fails the build. A table's file is `<UTC>_<schema>_<table>.sql`; a
//! later change to it is `<UTC>_<schema>_<table>_<what>.sql`.
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
//! set depend on the state of a working tree. The list is explicit, and the
//! crate's build script writes the `schema/*.sql` names into `OUT_DIR` for
//! `tests/migrations.rs` to compare against it — so a file added to `schema/`
//! and missing here fails `make test-unit-all` rather than a production
//! migrate. A listed file that does not exist fails `include_str!`, and one
//! listed twice claims its slot twice, which the ordering test refuses.

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
            sql: include_str!(concat!("../../../../../schema/", $file)),
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
pub const fn version_from_name(name: &str) -> i64 {
    version_of(name)
}

/// Every migration this binary knows, in application order.
///
/// Order is ascending version, which is also foreign-key order: a table follows
/// every table it references, across schemas. A foreign key that closes a
/// cycle (a head pointer) is the one `ALTER` a file holds, in the file of the
/// cycle's later table.
pub static MIGRATIONS: &[Migration] = &[
    migration!("20261108120000_schemas.sql"),
    migration!("20261108120100_roles_and_privileges.sql"),
    migration!("20261108120200_identity_tenants.sql"),
    migration!("20261108120300_identity_workspaces.sql"),
    migration!("20261108120400_identity_users.sql"),
    migration!("20261108120500_identity_memberships.sql"),
    migration!("20261108120600_identity_invites.sql"),
    migration!("20261108120700_identity_api_keys.sql"),
    migration!("20261108120800_identity_cli_credentials.sql"),
    migration!("20261108120900_identity_user_preferences.sql"),
    migration!("20261108121000_vault_secrets.sql"),
    migration!("20261108121100_model_platform_models.sql"),
    migration!("20261108121200_model_platform_model_prices.sql"),
    migration!("20261108121300_model_catalogue_version.sql"),
    migration!("20261108121400_model_platform_defaults.sql"),
    migration!("20261108121500_model_tenant_models.sql"),
    migration!("20261108121600_model_tenant_selections.sql"),
    migration!("20261108121700_authoring_library_entries.sql"),
    migration!("20261108121800_authoring_library_revisions.sql"),
    migration!("20261108121900_fleet_fleets.sql"),
    migration!("20261108122000_fleet_revisions.sql"),
    migration!("20261108122100_connector_integration_grants.sql"),
    migration!("20261108122200_connector_installs.sql"),
    migration!("20261108122300_connector_channels.sql"),
    migration!("20261108122400_fleet_events.sql"),
    migration!("20261108122500_fleet_tool_calls.sql"),
    migration!("20261108122600_fleet_approval_gates.sql"),
    migration!("20261108122700_fleet_obligations.sql"),
    migration!("20261108122800_fleet_schedules.sql"),
    migration!("20261108122900_fleet_sessions.sql"),
    migration!("20261108123000_fleet_activity_counters.sql"),
    migration!("20261108123100_runner_runners.sql"),
    migration!("20261108123200_runner_leases.sql"),
    migration!("20261108123300_runner_fleet_slots.sql"),
    migration!("20261108123400_runner_events.sql"),
    migration!("20261108123500_runner_lifetime_counters.sql"),
    migration!("20261108123600_authoring_drafts.sql"),
    migration!("20261108123700_authoring_candidates.sql"),
    migration!("20261108123800_authoring_messages.sql"),
    migration!("20261108123900_authoring_attempts.sql"),
    migration!("20261108124000_authoring_dispatches.sql"),
    migration!("20261108124100_authoring_activity_receipts.sql"),
    migration!("20261108124200_authoring_answer_receipts.sql"),
    migration!("20261108124300_authoring_replay_receipts.sql"),
    migration!("20261108124400_billing_wallets.sql"),
    migration!("20261108124500_billing_authoring_report_receipts.sql"),
    migration!("20261108124600_billing_usage_lines.sql"),
    migration!("20261108124700_memory_entries.sql"),
    migration!("20261108124800_memory_context_checkpoints.sql"),
    migration!("20261108124900_fleet_task_items.sql"),
    migration!("20261108125000_fleet_tasks.sql"),
    migration!("20261108125100_fleet_admissions.sql"),
    migration!("20261108125200_fleet_task_inputs.sql"),
    migration!("20261108125300_model_decision_revisions.sql"),
    migration!("20261108125400_model_decision_prices.sql"),
    migration!("20261108125500_runner_decision_calls.sql"),
    migration!("20261108125600_runner_decision_payloads.sql"),
    migration!("20261108125700_authoring_edit_decisions.sql"),
    migration!("20261108125800_fleet_task_adoptions.sql"),
    migration!("20261108125900_fleet_task_effects.sql"),
    migration!("20261108130000_fleet_task_effect_states.sql"),
    migration!("20261108130100_fleet_task_interpretations.sql"),
    migration!("20261108130200_fleet_task_effect_payloads.sql"),
    migration!("20261108130300_fleet_task_input_receipts.sql"),
    migration!("20261108130400_fleet_task_effect_recovery.sql"),
    migration!("20261108130500_fleet_task_identity_keys.sql"),
    migration!("20261108130600_fleet_task_storage_quota.sql"),
    migration!("20261108130700_fleet_task_bases.sql"),
    migration!("20261108130800_fleet_task_checkpoints.sql"),
    migration!("20261108130900_fleet_task_checkpoint_pages.sql"),
    migration!("20261108131000_fleet_task_base_pages.sql"),
    migration!("20261108131100_fleet_task_storage_purges.sql"),
    migration!("20261108131200_fleet_task_objects.sql"),
    migration!("20261108131300_fleet_task_object_refs.sql"),
    migration!("20261108131400_fleet_task_object_pins.sql"),
    migration!("20261108131500_fleet_task_checkpoint_operations.sql"),
    migration!("20261108131600_fleet_task_waits.sql"),
    migration!("20261108131700_fleet_task_completions.sql"),
    migration!("20261108131800_fleet_task_completion_fanout.sql"),
    migration!("20261108131900_connector_person_links.sql"),
    migration!("20261108193000_connector_oauth_nonces.sql"),
];

/// Derives the version from the filename during constant evaluation: its UTC
/// `yyyymmddHHMMSS` timestamp, the one way a schema file is named.
///
/// # Panics
/// During constant evaluation, when the name does not open with fourteen digits
/// spelling a real UTC instant followed by `_`. Every call site is a `static`
/// initialiser, so a malformed filename fails the build and never the process.
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
        digits == 14,
        "schema filename must open with a yyyymmddHHMMSS timestamp"
    );
    assert!(
        matches!(rest, [b'_', ..]),
        "schema filename must separate its timestamp with an underscore"
    );
    let year = version / 10_000_000_000;
    let month = (version / 100_000_000).rem_euclid(100);
    let day = (version / 1_000_000).rem_euclid(100);
    let hour = (version / 10_000).rem_euclid(100);
    let minute = (version / 100).rem_euclid(100);
    let second = version.rem_euclid(100);
    assert!(year >= 2000, "schema timestamp year is before 2000");
    assert!(
        month >= 1 && month <= 12,
        "schema timestamp month is not 01-12"
    );
    assert!(day >= 1 && day <= 31, "schema timestamp day is not 01-31");
    assert!(hour <= 23, "schema timestamp hour is not 00-23");
    assert!(minute <= 59, "schema timestamp minute is not 00-59");
    assert!(second <= 59, "schema timestamp second is not 00-59");
    version
}
