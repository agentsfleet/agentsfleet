//! RULE STS, for the literals that fail SILENTLY.
//!
//! `schema/**.sql` has to spell some names itself: Postgres parses a trigger
//! body, a partial-index predicate and a `current_setting()` name at DDL time,
//! and none of the three can reference a Rust constant. RULES.md names that gap
//! and prescribes the remedy — a slot-grep pin test — and then cites
//! `schema_privilege_test.zig` as precedent. That file went with the Zig sunset
//! and got no Rust replacement, so the rule has been naming its own example as
//! an open violation ever since.
//!
//! # Why this one and not every literal
//!
//! A drifted `DEFAULT` shows up as wrong data, and somebody notices. A drifted
//! `current_setting()` name fails silently in the worst direction: the setting
//! the daemon sets and the setting the trigger reads stop being the same
//! string, every append-only trigger starts refusing the purge cascade, and a
//! personal-account erasure stops working with nothing red anywhere. Seven
//! schema files write this name and one Rust statement sets it.
//!
//! # Why it lives in `afd_wire`
//!
//! The constants do (`afd_wire::schema`), and this crate depends on no other
//! `afd_*` crate, so the pin cannot introduce a cycle. It reads `schema/` from
//! disk; `afd_db`'s `migrations.rs` separately pins that directory against the
//! embedded migration list, so what ships is what is checked here.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::path::{Path, PathBuf};

use afd_wire::schema::{GATE_PURGE_ENABLED, GATE_PURGE_SETTING};

/// How many schema files are expected to read the purge setting.
///
/// Named so a file that stops reading it is as loud as one that misspells it:
/// dropping the guard from a table is a change to what the cascade may delete,
/// and it should never happen quietly.
const FILES_READING_THE_SETTING: usize = 7;

/// The repository root, four levels up from this crate's manifest
/// (`rustd/crates/afd_wire` → `rustd/crates` → `rustd` → root).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the manifest sits four levels below the repository root")
        .to_path_buf()
}

/// Every `schema/*.sql` file, as (name, contents).
fn schema_files() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = std::fs::read_dir(repo_root().join("schema"))
        .expect("schema/ must exist")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
                .then(|| {
                    let name = path.file_name()?.to_string_lossy().into_owned();
                    let body = std::fs::read_to_string(&path).ok()?;
                    Some((name, body))
                })?
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(!files.is_empty(), "a scan matching nothing is not a pass");
    files
}

/// Every setting name `body` passes to `current_setting(`.
fn settings_named_in(body: &str) -> Vec<String> {
    body.split("current_setting('")
        .skip(1)
        .filter_map(|tail| tail.split('\'').next().map(str::to_owned))
        .collect()
}

/// Every `current_setting()` in the schema names the constant, in every file.
///
/// A rename on the Rust side alone leaves these seven strings behind, and the
/// only symptom is a cascade that quietly stops being allowed through.
#[test]
fn every_schema_setting_name_is_the_one_rust_spells() {
    let mut files_reading_it = 0_usize;
    let mut names_seen = 0_usize;

    for (name, body) in schema_files() {
        let named = settings_named_in(&body);
        if named.is_empty() {
            continue;
        }
        files_reading_it += 1;
        for setting in named {
            names_seen += 1;
            assert_eq!(
                setting, GATE_PURGE_SETTING,
                "schema/{name} reads a setting the daemon never sets. Rust sets \
                 `{GATE_PURGE_SETTING}` (afd_wire::schema), so this trigger's \
                 guard can never open and the purge cascade it protects will be \
                 refused with nothing failing loudly."
            );
        }
    }

    assert!(names_seen > 0, "a scan matching nothing is not a pass");
    assert_eq!(
        files_reading_it, FILES_READING_THE_SETTING,
        "the number of schema files guarding on the purge setting changed. \
         Adding one is fine — update the constant. Losing one silently widens \
         what a cascade may delete."
    );
}

/// The value the schema compares against is the one Rust writes.
///
/// The name matching is not enough on its own: `= 'on'` on one side and
/// `= 'true'` on the other fails exactly as silently.
#[test]
fn every_schema_guard_compares_against_the_value_rust_writes() {
    let expected =
        format!("current_setting('{GATE_PURGE_SETTING}', true) = '{GATE_PURGE_ENABLED}'");
    let mut checked = 0_usize;

    for (name, body) in schema_files() {
        if !body.contains("current_setting('") {
            continue;
        }
        checked += 1;
        assert!(
            body.contains(&expected),
            "schema/{name} guards on the purge setting but not with `{expected}` \
             — the daemon writes `{GATE_PURGE_ENABLED}` and a comparison against \
             anything else never matches."
        );
    }

    assert_eq!(checked, FILES_READING_THE_SETTING);
}
