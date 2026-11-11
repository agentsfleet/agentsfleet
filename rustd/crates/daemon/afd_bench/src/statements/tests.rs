use std::collections::BTreeMap;

use super::paths::{Path, by_path_with, costliest, per_lease};
use super::{QueryCost, QueryReading, READ_COST, StatementCost, StatementReading};

/// Statements the server had run before a window opened.
const EARLIER_STATEMENTS: u64 = 1_000;

/// A reading with the given tallies and flush count.
const fn reading(statements: u64, commits: u64, flushed: u64) -> StatementReading {
    StatementReading {
        statements,
        commits,
        flushed,
    }
}

#[test]
fn a_window_that_did_nothing_costs_nothing() {
    // The later reading saw exactly the earlier read and its own four flushes:
    // the cost of looking, and nothing else.
    let earlier = reading(100, 50, 4);
    let later = reading(100 + READ_COST + 4, 50 + READ_COST + 4, 4);

    assert_eq!(later.since(earlier), StatementCost::default());
}

#[test]
fn a_window_is_charged_what_it_ran_and_nothing_the_readings_ran() {
    let earlier = reading(EARLIER_STATEMENTS, 300, 2);
    // Three statements and one commit of real work, plus the earlier read and
    // six flushes this reading made across a pool that grew meanwhile.
    let later = reading(
        EARLIER_STATEMENTS + 3 + READ_COST + 6,
        300 + 1 + READ_COST + 6,
        6,
    );

    assert_eq!(
        later.since(earlier),
        StatementCost {
            statements: 3,
            commits: 1,
        }
    );
}

#[test]
fn an_earlier_reading_past_the_later_one_saturates_rather_than_wraps() {
    // Only a server restart resets the tallies, and a restart also severs the
    // lane's connections, which fails the run before this delta is written. So
    // the arm is defensive: it must not wrap into eighteen quintillion.
    let earlier = reading(5_000, 900, 3);
    let later = reading(10, 2, 3);

    assert_eq!(later.since(earlier), StatementCost::default());
}

/// A table of three fragments, the third overlapping the first's statement.
const TABLE: &[(&str, Path)] = &[
    ("FROM runner.fleet_slots", Path::Claim),
    ("FROM billing.wallets", Path::Gates),
    ("runner.fleet_slots", Path::Report),
];

fn cost(query: &str, calls: u64) -> QueryCost {
    QueryCost {
        query: query.to_owned(),
        calls,
    }
}

#[test]
fn statements_by_path_sum_to_total() {
    let costs = [
        cost("SELECT 1 FROM runner.fleet_slots WHERE x", 10),
        cost("SELECT 1 FROM billing.wallets", 15),
        cost("UPDATE runner.fleet_slots SET y", 5),
    ];
    let totals = by_path_with(TABLE, &costs);
    let per = per_lease(&totals, 10);
    let of = |path: Path| per.as_ref().and_then(|per| per.get(&path)).copied();

    assert_eq!(of(Path::Claim), Some(1.0));
    assert_eq!(of(Path::Gates), Some(1.5));
    assert_eq!(of(Path::Report), Some(0.5));
    assert_eq!(
        per.map(|per| per.values().sum::<f64>()),
        Some(3.0),
        "no call is lost or doubled"
    );
}

#[test]
fn unmatched_statement_counts_as_other() {
    let totals = by_path_with(TABLE, &[cost("INSERT INTO fleet.unknown VALUES ($1)", 7)]);

    assert_eq!(totals.get(&Path::Other).copied(), Some(7));
    assert_eq!(totals.values().sum::<u64>(), 7);
}

#[test]
fn shared_statement_counts_in_its_first_path() {
    // Both the Claim and the Report fragment match; the earlier entry wins.
    let totals = by_path_with(TABLE, &[cost("SELECT * FROM runner.fleet_slots", 4)]);

    assert_eq!(
        (
            totals.get(&Path::Claim).copied(),
            totals.get(&Path::Report).copied()
        ),
        (Some(4), Some(0))
    );
}

#[test]
fn no_leases_reports_no_ratio() {
    let totals: BTreeMap<Path, u64> = BTreeMap::from([(Path::Claim, 9)]);

    assert!(
        per_lease(&totals, 0).is_none(),
        "a ratio over zero leases would report infinity"
    );
}

#[test]
fn a_query_reading_charges_only_the_window() {
    let earlier = QueryReading::of(&[(1, 100, "SELECT a"), (2, 5, "SELECT b")]);
    let later = QueryReading::of(&[(1, 130, "SELECT a"), (2, 5, "SELECT b"), (3, 2, "SELECT c")]);

    let mut ran = later.since(&earlier);
    ran.sort_by(|left, right| left.query.cmp(&right.query));

    assert_eq!(ran, vec![cost("SELECT a", 30), cost("SELECT c", 2)]);
}

#[test]
fn the_costliest_statements_come_first() {
    let costs = [cost("a", 1), cost("b", 9), cost("c", 4)];

    let top: Vec<&str> = costliest(&costs, 2)
        .iter()
        .map(|c| c.query.as_str())
        .collect();

    assert_eq!(top, vec!["b", "c"]);
}

/// One M245 statement, read from the product's own file, and the path it
/// belongs to.
const M245_STATEMENTS: &[(&str, &str, Path)] = &[
    (
        "claim_admission_head.sql",
        afd_fleet::lease::sql::queue::CLAIM_HEAD,
        Path::Claim,
    ),
    (
        "select_gates.sql",
        afd_fleet::lease::sql::gates::SELECT_GATES,
        Path::Gates,
    ),
    (
        "insert_lease_with_event.sql",
        afd_fleet::lease::sql::lease::INSERT_LEASE_WITH_EVENT,
        Path::Issue,
    ),
    (
        "select_renewals.sql",
        afd_fleet::lease::sql::renew::SELECT_RENEWALS,
        Path::Meter,
    ),
    (
        "select_revision_documents.sql",
        afd_fleet::lease::sql::renew::SELECT_REVISION_DOCUMENTS,
        Path::Meter,
    ),
    (
        "renew_batch.sql",
        afd_fleet::lease::sql::renew::RENEW_BATCH,
        Path::Meter,
    ),
    (
        "commit_report.sql",
        afd_fleet::lease::sql::report_statement::COMMIT_REPORT,
        Path::Report,
    ),
    (
        "apply_report_notice.sql",
        afd_fleet::lease::sql::report_statement::APPLY_REPORT_NOTICE,
        Path::Report,
    ),
    (
        "claim_inputs.sql",
        afd_fleet::lease::sql::lease::CLAIM_INPUTS,
        Path::Fold,
    ),
];

/// `pg_stat_statements` keeps a statement's comments, so the fragments must
/// classify it with them and without: a fragment that matched only a comment
/// would break on a reworded one.
fn code_of(statement: &str) -> String {
    statement
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

// Value: protects=each statement M245 added or rewrote is counted in the path
// the chat projection gives it; fails_when=one lands in `other`, or in an
// older path whose fragment it also contains; why_new=§4-§8 replaced the
// statements the table was read off; seam=none; no_claim=statements a drain
// run shows that no file here names, which the run's `other` reports.
#[test]
fn new_statements_match_their_paths() {
    for (file, statement, path) in M245_STATEMENTS {
        assert_eq!(
            super::paths::classify(statement),
            *path,
            "{file} lands in its path"
        );
        assert_eq!(
            super::paths::classify(&code_of(statement)),
            *path,
            "{file} lands in its path without its comments"
        );
    }
}
