//! The migration list is the `schema/` directory, spelled out by hand.
//!
//! The integration half proves a fresh database ends up with the right rows.
//! This half proves the list those rows come from cannot drift from the files
//! it names. `afd_db::migration` writes every filename out rather than globbing
//! the directory — deliberately, so the set does not depend on the state of a
//! working tree — and a hand-written list is exactly what falls behind. A
//! `.sql` dropped into `schema/` without its `migration!()` entry fails here,
//! in the fast lane, rather than in production as a table nothing created.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afd_db::migration::{MIGRATIONS, Migration};

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

/// Two lists, one set: the files on disk and the ones this crate ships.
#[test]
fn test_migration_list_matches_schema_directory() {
    let ours: BTreeSet<String> = MIGRATIONS
        .iter()
        .map(|migration| migration.name().to_owned())
        .collect();
    let directory = schema_directory();

    assert_eq!(
        ours, directory,
        "the migration list and schema/ disagree — a file was added or removed without updating src/migration.rs"
    );
    assert_eq!(MIGRATIONS.len(), ours.len(), "a filename is listed twice");
}

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
            prefix.parse::<i32>().unwrap(),
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
    let versions: Vec<i32> = MIGRATIONS.iter().map(Migration::version).collect();
    let mut sorted = versions.clone();
    sorted.sort_unstable();
    assert_eq!(versions, sorted, "the list is out of dependency order");

    let distinct: BTreeSet<i32> = versions.iter().copied().collect();
    assert_eq!(distinct.len(), versions.len(), "two files claim one slot");
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

/// `schema/720` no longer justifies its index by a reader it lost.
///
/// The fleet index led with `fleet_id` because the `SET NULL` referential
/// action matched on that column alone. Slot 915 drops that foreign key, so
/// the justification went with it while the index stayed. A comment that still
/// cites the retired reader is worse than no comment: the next person to weigh
/// reordering or dropping this index would weigh it against a constraint that
/// no longer exists.
///
/// Asserted against the file rather than left to review, because this is
/// exactly the kind of prose that drifts back on a careless revert.
#[test]
fn test_m201_index_comment_names_surviving_reader() {
    let indexes = std::fs::read_to_string(repo_root().join("schema/720_usage_ledger_indexes.sql"))
        .expect("schema/720 must exist");
    assert!(
        !indexes.contains("Reader 2 — the fleet SET NULL"),
        "schema/720 still cites the SET NULL reader that slot 915 removed"
    );
    assert!(
        indexes.contains("schema/915"),
        "schema/720 must name the slot that removed its second reader, so the \
         history is followable from the file that changed meaning"
    );
}

/// Slot 915 drops the foreign key by lookup, not by guessed name.
///
/// `DROP CONSTRAINT IF EXISTS usage_ledger_fleet_id_fkey` is the tempting
/// spelling and the dangerous one: `IF EXISTS` swallows a name miss, so a
/// generated name that differs by even one character leaves the foreign key in
/// place while the migration reports success. The whole point of the slot is
/// that `ON DELETE SET NULL` stops firing, and that failure mode is silent.
///
/// The catalogue lookup cannot miss that way, so this test pins the shape.
#[test]
fn test_m201_slot_915_drops_the_constraint_by_lookup() {
    let slot = MIGRATIONS
        .iter()
        .find(|m| m.version() == 915)
        .expect("slot 915 must be registered");

    // Comments stripped first. The slot's own prose explains why the `IF
    // EXISTS` form is wrong, and that explanation contains the very string
    // this asserts the absence of — so a whole-file grep fails on the
    // documentation rather than on the code. Ask the question of the
    // statements only.
    let statements: String = slot
        .sql()
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        statements.contains("pg_constraint"),
        "slot 915 must find the foreign key in the catalogue, not guess its name"
    );
    assert!(
        !statements.contains("DROP CONSTRAINT IF EXISTS"),
        "slot 915 must not use the IF EXISTS form, which hides a name miss"
    );
}

/// Slot 915 is in the shipped list exactly once, and it follows 914.
///
/// The list is hand-written (see this file's module note), so a slot whose
/// `.sql` landed without its `migration!()` entry is the failure mode this
/// crate's other tests already catch. What they do NOT catch is the pair of
/// mistakes that are specific to appending: a duplicated entry, which applies
/// the same file twice and makes the ledger disagree with the directory, and an
/// entry inserted ABOVE an already-shipped slot, which would renumber nothing
/// but would run this file before the table it alters exists on a fresh
/// database while leaving an upgraded one untouched — a divergence that only
/// shows up on the next clean install.
#[test]
fn test_m201_migration_slot_registered() {
    const LEDGER_IDENTITY: &str = "915_usage_ledger_retains_fleet_identity.sql";

    let listed: Vec<&Migration> = MIGRATIONS
        .iter()
        .filter(|migration| migration.name() == LEDGER_IDENTITY)
        .collect();
    assert_eq!(
        listed.len(),
        1,
        "{LEDGER_IDENTITY} must be registered exactly once"
    );

    let position = MIGRATIONS
        .iter()
        .position(|migration| migration.name() == LEDGER_IDENTITY)
        .expect("the slot was just found by name");
    let predecessor = MIGRATIONS
        .get(position.wrapping_sub(1))
        .expect("slot 915 is never the first entry");
    assert_eq!(
        predecessor.version(),
        914,
        "slot 915 must follow 914 — an entry placed above a shipped slot runs \
         in a different order on a fresh database than on an upgraded one"
    );
}
