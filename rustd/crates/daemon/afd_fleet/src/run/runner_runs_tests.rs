//! Each run is counted once, on its own runner, under how it ended; and no
//! statement keeps a tally of its own in Postgres.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]

use std::path::{Path, PathBuf};

use af_observability::test_util::Capture as Metrics;
use afd_db::test_util::mint_id;

use super::{acquired, expired, reported};

/// The family, as an operator's query names it.
const RUNS: &str = "agentsfleet_runner_runs_total";

/// Every outcome the family carries, as its label spells it.
const ACQUIRED: &str = "acquired";
const SUCCEEDED: &str = "succeeded";
const FAILED: &str = "failed";
const EXPIRED: &str = "expired";

/// The count one runner holds under one outcome.
///
/// Each test mints its own runner, and the capture reads cumulative totals,
/// so the count is this test's alone whatever else the binary records.
fn count(metrics: &Metrics, runner_id: &str, outcome: &str) -> u64 {
    metrics.sum(RUNS, &[("runner_id", runner_id), ("outcome", outcome)])
}

/// A run that succeeds, one that fails and one that expires each count once on
/// their runner, under their own outcome, and on no other runner.
#[test]
fn a_settled_run_counts_its_outcome_on_its_runner() {
    let metrics = Metrics::install();
    let (runner, bystander) = (mint_id(), mint_id());

    reported(&runner, true);
    reported(&runner, false);
    expired(&runner);

    let id = runner.as_str();
    assert_eq!(count(&metrics, id, SUCCEEDED), 1, "the clean finish");
    assert_eq!(count(&metrics, id, FAILED), 1, "the failed report");
    assert_eq!(count(&metrics, id, EXPIRED), 1, "the lapsed run");
    assert_eq!(
        count(&metrics, id, ACQUIRED),
        0,
        "ending a run acquires nothing"
    );
    for outcome in [ACQUIRED, SUCCEEDED, FAILED, EXPIRED] {
        assert_eq!(
            count(&metrics, bystander.as_str(), outcome),
            0,
            "a run counts on its own runner, never on another's ({outcome})"
        );
    }
}

/// An issued run counts acquired and no ending: it has not ended yet.
#[test]
fn an_issued_run_counts_acquired_and_no_ending() {
    let metrics = Metrics::install();
    let runner = mint_id();

    acquired(&runner);

    let id = runner.as_str();
    assert_eq!(count(&metrics, id, ACQUIRED), 1);
    for outcome in [SUCCEEDED, FAILED, EXPIRED] {
        assert_eq!(
            count(&metrics, id, outcome),
            0,
            "an issue is not an ending ({outcome})"
        );
    }
}

/// The table the counter replaced, built so this file does not spell it.
fn retired_table() -> String {
    ["runner.", "lifetime", "_counters"].concat()
}

/// `rustd/`, three levels above this crate's manifest.
fn rustd_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("afd_fleet sits at rustd/crates/daemon/afd_fleet")
        .to_path_buf()
}

/// Every `.sql` file under `root`, skipping build output and hidden folders.
fn statements(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut found = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("a readable source folder") {
            let path = entry.expect("a readable source entry").path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "sql") {
                found.push(path);
            }
        }
    }
    found
}

/// The statements under `rustd/` whose text names `table`, sorted.
fn naming(table: &str) -> Vec<PathBuf> {
    let mut named: Vec<PathBuf> = statements(&rustd_root())
        .into_iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .expect("a readable statement")
                .contains(table)
        })
        .collect();
    named.sort();
    named
}

/// No schema slot or statement under `rustd/` names the retired tally table.
///
/// The walk is proved on a known positive first: the report statement names
/// `runner.history`, so a walk that reached nothing could not pass.
#[test]
fn no_statement_names_the_retired_tally_table() {
    let history = naming("runner.history");
    assert!(
        history
            .iter()
            .any(|path| path.ends_with("run/sql/commit_report.sql")),
        "the walk must reach the report statement, which names runner.history: {history:?}"
    );

    let retired = retired_table();
    let still = naming(&retired);
    assert!(
        still.is_empty(),
        "these statements still name {retired}, whose counts are now a metric: {still:?}"
    );
}
