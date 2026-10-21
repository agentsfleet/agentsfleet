//! The migration list, held to the shape the migrator relies on.
//!
//! The integration half proves a fresh database ends up with the right rows.
//! This half proves the hand-written list in `afd_db::migration` is ordered,
//! one slot per file, each applying as its filename's number. That every
//! tracked `schema/*.sql` has an entry is checked by a lint
//! (`scripts/check_rust_source.py`, rule `rust-migration-list`), which reads
//! both listings without running anything.
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use afd_db::migration::{MIGRATIONS, Migration};

/// The version is the slot number, derived rather than restated.
///
/// `afd_db::migration` derives it from the filename during constant
/// evaluation, so a version that disagrees with the file it names is not a
/// mistake anyone can write. This walks the derivation over every committed
/// filename, which puts that compile-time guarantee in the test output where a
/// reader can see it hold.
#[test]
fn test_every_version_is_its_filename_prefix() {
    for migration in MIGRATIONS {
        let prefix: String = migration
            .name()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        assert_eq!(
            prefix.parse::<i64>().unwrap(),
            migration.version(),
            "{} does not apply as its slot number",
            migration.name()
        );
    }
}

/// Applied in ascending version order, which is dependency order: `1xx`
/// substrate before `2xx` identity before `5xx` fleets, because that is the
/// order an empty database must create them in.
#[test]
fn test_migrations_are_in_ascending_version_order() {
    let versions: Vec<i64> = MIGRATIONS.iter().map(Migration::version).collect();
    let mut sorted = versions.clone();
    sorted.sort_unstable();
    assert_eq!(versions, sorted, "the list is out of dependency order");

    let distinct: BTreeSet<i64> = versions.iter().copied().collect();
    assert_eq!(distinct.len(), versions.len(), "two files claim one slot");
}

/// The slot-number derivation, run rather than compiled.
///
/// Every production call site is a `static` initialiser, so the build proves
/// every real filename derives correctly and nothing proves what the function
/// does with anything else. `version_from_name` is the same body called at
/// runtime, which is where the edges live: a name with no digits, a name whose
/// digits are not followed by an underscore, a number wider than the slots in
/// use.
#[test]
fn test_version_derivation_reads_the_leading_slot_number() {
    for (name, expected) in [
        ("100_schemas.sql", 100),
        ("0_zero.sql", 0),
        ("890_fleet_activity_counter_triggers.sql", 890),
        ("9223372036854775807_max.sql", i64::MAX),
    ] {
        assert_eq!(
            afd_db::migration::version_from_name(name),
            expected,
            "{name} derived the wrong slot"
        );
    }
}

/// A filename the derivation refuses is a BUILD failure, and this is what that
/// refusal looks like when the same code runs at runtime.
///
/// Checked through a child process, because the refusal is a panic: in a
/// `static` initialiser it fails the build, which is the point, and there is no
/// other way to observe the same branch.
#[test]
fn test_version_derivation_refuses_a_name_without_a_slot_number() {
    for bad in ["schemas.sql", "100schemas.sql", "_100.sql", ""] {
        let refused = std::panic::catch_unwind(|| afd_db::migration::version_from_name(bad));
        assert!(
            refused.is_err(),
            "{bad:?} was accepted — a schema file with no slot number would apply as version 0"
        );
    }
}

/// The default migrator is the canonical one.
///
/// `Default` exists so a caller can write `Migrator::default()`, and the risk
/// of a hand-written `Default` is that it quietly diverges from `new()` — a
/// migrator running a DIFFERENT list than the one this crate ships.
#[test]
fn test_the_default_migrator_runs_the_canonical_list() {
    let canonical: Vec<i64> = MIGRATIONS.iter().map(Migration::version).collect();
    assert_eq!(afd_db::Migrator::default().canonical_versions(), canonical);
    assert_eq!(
        afd_db::Migrator::default().canonical_versions(),
        afd_db::Migrator::new().canonical_versions(),
        "Default and new() must not describe two different migrators"
    );
}

/// The adapter hands `SQLx` the compiled migration unchanged, in one transaction.
#[test]
fn test_every_migration_adapter_preserves_sql_identity_and_transaction() {
    for migration in MIGRATIONS {
        let applied = migration.to_sqlx();
        assert_eq!(applied.version, migration.version());
        assert_eq!(applied.description, migration.name());
        assert_eq!(applied.sql, migration.sql());
        assert_eq!(applied.migration_type, sqlx::migrate::MigrationType::Simple);
        assert!(!applied.checksum.is_empty());
        assert!(!applied.no_tx);
    }
}
