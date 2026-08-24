//! Dimension 2.1 (list half) — the Rust migration list IS the Zig one.
//!
//! The integration half proves a fresh database ends up with the right rows.
//! This half proves the list those rows come from cannot drift: it is compared
//! against the `schema/` directory AND against `schema/embed.zig`, which is
//! the Zig daemon's own source of truth. A migration added to either side and
//! not the other fails here, in the fast lane, rather than in production as a
//! table that never got created.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afd_db::migration::{MIGRATIONS, Migration};
use afd_db::sql::SqlStatements;

/// The repository root, four levels up from this crate's manifest
/// (`rustd/crates/afd_db` → `rustd/crates` → `rustd` → root).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// The `.sql` files on disk, by name.
fn schema_directory() -> BTreeSet<String> {
    std::fs::read_dir(repo_root().join("schema"))
        .expect("schema/ must exist")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
                .then_some(name)
        })
        .collect()
}

/// The files `schema/embed.zig` lists, by name.
fn zig_embedded_files() -> BTreeSet<String> {
    let source = std::fs::read_to_string(repo_root().join("schema/embed.zig"))
        .expect("schema/embed.zig must exist");
    source
        .split("@embedFile(\"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next().map(str::to_owned))
        .collect()
}

/// Three lists, one set: the directory, the Zig daemon's list, and this crate's.
#[test]
fn test_migration_list_matches_schema_directory_and_zig() {
    let ours: BTreeSet<String> = MIGRATIONS
        .iter()
        .map(|migration| migration.name().to_owned())
        .collect();
    let directory = schema_directory();
    let zig = zig_embedded_files();

    assert_eq!(
        ours, directory,
        "the Rust list and schema/ disagree — a file was added or removed without updating src/migration.rs"
    );
    assert_eq!(
        ours, zig,
        "the Rust list and schema/embed.zig disagree — the two binaries would migrate to different schemas"
    );
    assert_eq!(MIGRATIONS.len(), ours.len(), "a filename is listed twice");
}

/// The version is the slot number, derived rather than restated.
///
/// This is the property `schema/embed.zig` documents as RULE MIG and then
/// maintains by hand. Here it is derived at compile time, so the test is
/// checking the derivation against the filenames rather than watching for a
/// typo — but the Zig side is still hand-written, so its numbers are compared
/// too.
#[test]
fn test_every_version_is_its_filename_prefix() {
    let source = std::fs::read_to_string(repo_root().join("schema/embed.zig")).unwrap();

    for migration in MIGRATIONS {
        let prefix: String = migration
            .name()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        assert_eq!(
            prefix.parse::<i32>().unwrap(),
            migration.version(),
            "{} does not apply as its slot number",
            migration.name()
        );

        let zig_row = format!(
            ".version = {}, .sql = @embedFile(\"{}\")",
            migration.version(),
            migration.name()
        );
        assert!(
            source.contains(&zig_row),
            "schema/embed.zig binds {} to a different version than {}",
            migration.name(),
            migration.version()
        );
    }
}

/// Applied in ascending version order, which is dependency order: `1xx`
/// substrate before `2xx` identity before `5xx` fleets, because that is the
/// order an empty database must create them in.
#[test]
fn test_migrations_are_in_ascending_version_order() {
    let versions: Vec<i32> = MIGRATIONS.iter().map(Migration::version).collect();
    let mut sorted = versions.clone();
    sorted.sort_unstable();
    assert_eq!(versions, sorted, "the list is out of dependency order");

    let distinct: BTreeSet<i32> = versions.iter().copied().collect();
    assert_eq!(distinct.len(), versions.len(), "two files claim one slot");
}

/// Every committed migration splits into applicable statements.
///
/// The corpus guard: a migration that ends inside a string literal or a
/// function body would be refused at apply time, in the middle of a deploy,
/// against a half-migrated database. It is refused here instead, on every
/// unit-test run, before it is ever committed.
#[test]
fn test_every_migration_splits_into_statements() {
    for migration in MIGRATIONS {
        let statements = migration
            .statements()
            .unwrap_or_else(|error| panic!("{} is malformed: {error}", migration.name()));
        let count = statements.count();
        assert!(
            count > 0,
            "{} contains no statements — an empty migration still takes a version",
            migration.name()
        );
    }
}

/// No migration is empty, and each one's SQL is the file's own content.
#[test]
fn test_every_migration_carries_its_file() {
    for migration in MIGRATIONS {
        let on_disk = std::fs::read_to_string(repo_root().join("schema").join(migration.name()))
            .unwrap_or_else(|error| panic!("{} is not readable: {error}", migration.name()));
        assert_eq!(
            migration.sql(),
            on_disk,
            "{} was embedded from somewhere other than schema/",
            migration.name()
        );
    }
}

/// The whole corpus is structurally sound as one scan, which is the shape the
/// migrator relies on when it refuses a file before applying any of it.
#[test]
fn test_the_corpus_is_structurally_sound() {
    for migration in MIGRATIONS {
        assert!(
            SqlStatements::new(migration.sql()).is_ok(),
            "{} would be refused at apply time",
            migration.name()
        );
    }
}

/// The slot-number derivation, run rather than compiled.
///
/// Every production call site is a `static` initialiser, so the build proves
/// the 47 real filenames derive correctly and nothing proves what the function
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
        ("2147483647_max.sql", i32::MAX),
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

/// `Migration::for_test` carries exactly what it was handed.
///
/// It is a `const fn`, and every production caller is a `static` initialiser —
/// which is why it needs a test that calls it at RUNTIME. A const evaluated at
/// compile time proves the compiler agrees with itself; the failure-bookkeeping
/// proof builds one of these from values it computes, and that is this path.
#[test]
#[cfg(feature = "test-util")]
fn test_a_test_only_migration_carries_what_it_was_given() {
    let version = 9_999_i32;
    let migration = Migration::for_test(version, "9999_not_committed.sql", "SELECT 1;");

    assert_eq!(migration.version(), version);
    assert_eq!(migration.name(), "9999_not_committed.sql");
    assert_eq!(migration.sql(), "SELECT 1;");
    assert!(
        !MIGRATIONS.iter().any(|m| m.version() == version),
        "a test-only migration must not collide with a committed slot"
    );
}

/// The default migrator is the canonical one.
///
/// `Default` exists so a caller can write `Migrator::default()`, and the risk
/// of a hand-written `Default` is that it quietly diverges from `new()` — a
/// migrator running a DIFFERENT list than the one this crate ships.
#[test]
fn test_the_default_migrator_runs_the_canonical_list() {
    let canonical: Vec<i32> = MIGRATIONS.iter().map(Migration::version).collect();
    assert_eq!(afd_db::Migrator::default().canonical_versions(), canonical);
    assert_eq!(
        afd_db::Migrator::default().canonical_versions(),
        afd_db::Migrator::new().canonical_versions(),
        "Default and new() must not describe two different migrators"
    );
}
