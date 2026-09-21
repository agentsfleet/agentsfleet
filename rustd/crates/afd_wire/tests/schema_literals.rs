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
// 912 replaces `repair_verifications_fenced_update` to drop the once-key
// cleanup arm, and a replacement carries the purge guard forward with it — so
// the setting is now spelled in the 835 original and the 912 replacement alike.
const FILES_READING_THE_SETTING: usize = 8;

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

/// How many statements in the workspace insert into the usage ledger.
///
/// Named for the same reason `FILES_READING_THE_SETTING` is: a writer that
/// DISAPPEARS should be as loud as one that drifts. Three today — the renewal
/// accumulate, the report accumulate, and the receive insert.
const LEDGER_WRITERS: usize = 3;

/// The arbiter every ledger writer must name, from slot 916 onward.
const LEDGER_ARBITER: &str = "event_id, charge_type, fleet_id";

/// Every `crates/*/src/**.rs` file's body, comment lines removed.
///
/// Comments are stripped rather than searched around because several of these
/// modules explain the conflict arm in prose directly above the statement that
/// carries it, and a scan that counted the prose would pass while the statement
/// beneath it said something else.
fn rust_sources_without_comments() -> Vec<(String, String)> {
    fn walk(directory: &Path, found: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(body) = std::fs::read_to_string(&path)
            {
                let code = body
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                found.push((path.display().to_string(), code));
            }
        }
    }

    let mut found = Vec::new();
    let crates = repo_root().join("rustd").join("crates");
    for entry in std::fs::read_dir(&crates)
        .expect("rustd/crates must exist")
        .flatten()
    {
        walk(&entry.path().join("src"), &mut found);
    }
    assert!(!found.is_empty(), "a scan matching nothing is not a pass");
    found
}

/// Every insert into `billing.usage_ledger` arbitrates on the fleet too.
///
/// The conflict target is where a money invariant is actually enforced, and it
/// is spelled once per writer with nothing binding the three spellings
/// together. Slot 916 narrowed the key; a writer still naming
/// `(event_id, charge_type)` would not fail loudly against it — PostgreSQL
/// raises "no unique or exclusion constraint matching" only at execution, on a
/// path that runs when money moves. This is the check that fires first.
///
/// An `ON CONFLICT` is attributed to the table of the `INSERT INTO` that
/// precedes it, so a statement inserting elsewhere in the same file is not
/// swept in.
#[test]
fn every_ledger_conflict_target_carries_the_fleet() {
    let mut writers = Vec::new();

    for (name, body) in rust_sources_without_comments() {
        for (offset, _) in body.match_indices("ON CONFLICT (") {
            let Some(insert) = body[..offset].rfind("INSERT INTO ") else {
                continue;
            };
            let table = body[insert + "INSERT INTO ".len()..]
                .split(|c: char| c.is_whitespace() || c == '(')
                .next()
                .unwrap_or_default();
            if table != "billing.usage_ledger" {
                continue;
            }
            let target = body[offset + "ON CONFLICT (".len()..]
                .split(')')
                .next()
                .expect("a conflict target closes its parenthesis")
                .to_owned();
            writers.push((name.clone(), target));
        }
    }

    for (name, target) in &writers {
        assert_eq!(
            target, LEDGER_ARBITER,
            "{name} arbitrates the ledger on ({target}), not the fleet-scoped key"
        );
    }
    assert_eq!(
        writers.len(),
        LEDGER_WRITERS,
        "the ledger's writers changed in number: {writers:#?}"
    );
}

/// Phrases that treat `event_id` alone as the ledger's identity.
///
/// Each is a sentence a reader acts on rather than a constraint they copy, so
/// none contains the retired parenthesised spelling and none is caught by the
/// check above. They are the shapes that were actually on the page.
const RETIRED_IDENTITY_CLAIMS: [&str; 4] = [
    "join key `event_id`",
    "via `event_id`",
    "rows per event ",
    "rows per event|",
];

/// The architecture pages that state the ledger's key, and must state it right.
///
/// `name_architecture` makes these pages authoritative until reconciled, which
/// cuts both ways: a page naming a key the schema retired is not stale
/// documentation, it is the canonical answer being wrong. An agent consulting
/// it would design against `(event_id, charge_type)` and be told by the
/// operating model that the page wins.
const PAGES_NAMING_THE_LEDGER_KEY: [&str; 2] = [
    "docs/architecture/data_flow.md",
    "docs/architecture/billing_and_provider_keys.md",
];

/// No architecture page still names the arbiter slot 916 retired.
#[test]
fn architecture_pages_name_the_composite_ledger_key() {
    for page in PAGES_NAMING_THE_LEDGER_KEY {
        let path = repo_root().join(page);
        let body = std::fs::read_to_string(&path)
            .map_err(|error| format!("{page} must be readable: {error}"))
            .expect("an architecture page this rule names must exist");

        assert!(
            body.contains(LEDGER_ARBITER),
            "{page} must name the fleet-scoped arbiter ({LEDGER_ARBITER})"
        );
        // The retired spelling must not survive anywhere on the page. The
        // composite never matches this substring — `(fleet_id, ` sits where
        // the opening parenthesis would be — so any hit is a bare mention, and
        // one is enough: a page that adds the new key in one paragraph and
        // leaves the old one standing three paragraphs down still misleads.
        let retired: Vec<&str> = body
            .lines()
            .filter(|line| line.contains("(event_id, charge_type)"))
            .collect();
        assert!(
            retired.is_empty(),
            "{page} still names the retired ledger key: {retired:#?}"
        );

        // The constraint spelling is not where a page misleads. This pin
        // passed green on a `data_flow.md` that carried the new UNIQUE and,
        // three hundred lines down, still called `event_id` the join key and
        // told a reader the ledger joins to `fleet_events` through it — the
        // exact aggregation slot 916 exists to stop. A reviewer caught what
        // this test did not, so the test now reads the identity claim too.
        let event_only: Vec<&str> = body
            .lines()
            .filter(|line| {
                RETIRED_IDENTITY_CLAIMS
                    .iter()
                    .any(|claim| line.contains(claim))
            })
            .collect();
        assert!(
            event_only.is_empty(),
            "{page} still treats `event_id` alone as the ledger's identity. \
             Two fleets may hold one event id, so a join or a count on it \
             merges their money: {event_only:#?}"
        );
    }
}
