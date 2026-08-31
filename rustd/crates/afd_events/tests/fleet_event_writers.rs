//! Who is allowed to write the durable fleet-event row.
//!
//! # Why this is a source scan and not a row count
//!
//! The invariant is a NEGATIVE about a whole workspace — "ingress writes no
//! durable row" — and no runtime assertion can prove a negative about code that
//! did not run. A test that drove one ingress path and counted zero rows would
//! pass while a second, untested path inserted freely. The set of call sites is
//! the thing that is actually bounded, so the set is what gets asserted.
//!
//! `afd_ingress/src/deliver.rs:11` and the M180 spec's §2 Correction both state
//! this roster in prose. Until now that is all they did.
//!
//! # What breaks if ingress writes one
//!
//! The durable row appears when the RUNNER leases the event. A daemon that also
//! inserted at ingress would be racing its own runner to describe the same
//! event, and the two descriptions do not agree — at ingress there is no lease,
//! no runner, and no outcome to record. What makes a redelivery safe is
//! `afd_redis::streams::OnceScope`'s claim key, set in the same Lua script as
//! the `XADD`, so the claim and the append cannot come apart. A Postgres write
//! at ingress sits outside that script and outside its guarantee.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "structural test setup must stop at the first unreadable source tree"
)]

use std::fs;
use std::path::{Path, PathBuf};

/// The constant every fleet-event insert goes through.
const QUERY: &str = "INSERT_FLEET_EVENT";

/// The crate that DECLARES the constant, which is not a use of it.
const DECLARING_CRATE: &str = "afd_events";

/// Every crate permitted to execute it, and why it holds the right.
const WRITERS: [(&str, &str); 2] = [
    (
        "afd_fleet",
        "the runner leasing an event — the durable row's author",
    ),
    (
        "afd_approval",
        "the approval inbox recording a resolved gate",
    ),
];

/// The crate this roster exists to keep OUT, named so the failure is legible.
const INGRESS_CRATE: &str = "afd_ingress";

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("afd_events must remain under rustd/crates")
        .to_owned()
}

/// Every `.rs` file under `dir`, recursively.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// True when the line uses the constant rather than talking about it.
///
/// Doc comments name it in `afd_events` and `afd_ingress` precisely to explain
/// this rule, so a scan that counted prose would report the two files that
/// document the invariant as the two that violate it.
fn is_a_use(line: &str) -> bool {
    let code = line.trim_start();
    !code.starts_with("//") && !code.starts_with('*') && code.contains(QUERY)
}

/// Every crate with a non-comment reference to the query, with its call site.
fn callers() -> Vec<(String, String)> {
    let root = crates_root();
    let mut found = Vec::new();
    let entries = fs::read_dir(&root).expect("crates/ is readable");
    for entry in entries {
        let crate_dir = entry.expect("a readable crate entry").path();
        let src = crate_dir.join("src");
        if !src.is_dir() {
            continue;
        }
        let name = crate_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a crate directory has a UTF-8 name")
            .to_owned();
        if name == DECLARING_CRATE {
            continue;
        }
        let mut files = Vec::new();
        sources(&src, &mut files);
        for file in files {
            let body = fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
            for (offset, line) in body.lines().enumerate() {
                if is_a_use(line) {
                    let where_ = format!(
                        "{}:{}",
                        file.strip_prefix(&root).unwrap_or(&file).display(),
                        offset + 1
                    );
                    found.push((name.clone(), where_));
                }
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_durable_event_row_has_exactly_two_writers_and_ingress_is_not_one() {
    let found = callers();

    let mut crates: Vec<&str> = found.iter().map(|(c, _)| c.as_str()).collect();
    crates.dedup();

    let mut permitted: Vec<&str> = WRITERS.iter().map(|(c, _)| *c).collect();
    permitted.sort_unstable();

    assert!(
        !crates.contains(&INGRESS_CRATE),
        "{INGRESS_CRATE} executes {QUERY} at {:?}. Ingress writes NOTHING to \
         Postgres: the durable row is the runner's, written when it leases the \
         event. Append to the stream through `afd_redis::streams::OnceScope` — \
         the claim key and the XADD share one Lua script, which is what makes a \
         redelivery safe. A row written here is outside that guarantee and \
         races the runner's own description of the same event.",
        found
            .iter()
            .filter(|(c, _)| c == INGRESS_CRATE)
            .map(|(_, w)| w.as_str())
            .collect::<Vec<_>>()
    );

    assert_eq!(
        crates, permitted,
        "the roster of crates writing the durable fleet-event row changed.\n\
         found:     {found:#?}\n\
         permitted: {WRITERS:#?}\n\
         A new writer needs a reason recorded in WRITERS above, and a reader of \
         `afd_events::sql` needs to be able to find it. A removed one means this \
         roster is stale."
    );
}
