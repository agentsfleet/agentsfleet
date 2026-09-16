//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a predicate that answers for the wrong kind, only
//! shows up while something else is already going wrong.
//!
//! # What this crate has that the others do not
//!
//! `is_datastore_unavailable` and `is_over_capacity` deliberately OVERLAP — both
//! answer 503 — and `is_over_capacity` is how an operator tells a full queue from
//! a dead one. The overlap is the invariant, so it is pinned here by count.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a kind added without a sample, or a lift swapped for a `to_string()`,
//! fails here instead of reaching an operator as a broken chain.

#![cfg(feature = "test-util")]

use std::collections::BTreeSet;
use std::error::Error as _;

use afd_admission::error::one_of_each_kind;

/// How many kinds the sample declares.
const SAMPLES: usize = 9;
/// How many carry a `source()` — the chain an operator follows to the cause.
const WITH_SOURCE: usize = 8;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 3;

#[test]
fn the_sample_declares_every_kind_under_a_distinct_label() {
    let kinds = one_of_each_kind();
    assert_eq!(
        kinds.len(),
        SAMPLES,
        "a kind was added to ErrorKind without a sample beside it, or a sample \
         was removed — the suites that grade this surface only see what is here"
    );

    let labels: BTreeSet<&str> = kinds.iter().map(|(label, _)| *label).collect();
    assert_eq!(
        labels.len(),
        kinds.len(),
        "two samples share a label, so a failure here would name the wrong kind"
    );
    assert!(
        labels.iter().all(|label| !label.trim().is_empty()),
        "a blank label makes an assertion failure unreadable"
    );
}

#[test]
fn every_kind_renders_leading_with_its_code() {
    for (label, error) in one_of_each_kind() {
        let rendered = error.to_string();
        let code = error.code().as_str();
        assert!(
            rendered.starts_with(&format!("[{code}]")),
            "{label} does not lead with its code: {rendered}"
        );
        assert!(
            rendered.len() > code.len() + 3,
            "{label} renders its code and nothing else, so the row says nothing"
        );
    }
}

/// No error reports ITSELF as its own cause (`RUST_ERROR_STANDARD` rule 4).
///
/// A kind returned as its own `source()` prints the same sentence twice to any
/// `{:#}` walker before reaching anything new, which reads as a truncated chain
/// to whoever is following it.
#[test]
fn no_error_reports_itself_as_its_own_cause() {
    for (label, error) in one_of_each_kind() {
        let rendered = error.to_string();
        if let Some(source) = error.source() {
            assert_ne!(
                source.to_string(),
                rendered,
                "{label} reports itself as its own cause"
            );
            assert!(
                !rendered.ends_with(&source.to_string()),
                "{label} repeats its source verbatim: {rendered}"
            );
        }
    }
}

/// The wrapped failure survives `?` rather than being flattened into a string.
///
/// Pinned by COUNT because the failure mode is silent: `map_err(|e|
/// Mine(e.to_string()))` compiles, reads the same at the call site, and destroys
/// the chain an operator follows to the real cause.
#[test]
fn the_chain_survives_for_every_kind_that_has_one() {
    let wrapped: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.source().is_some())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        wrapped.len(),
        WITH_SOURCE,
        "{wrapped:?} keep a source; a lift was replaced by a stringify, or a \
         sample stopped carrying the cause it was built to carry"
    );
}

#[test]
fn every_code_is_one_the_registry_declares() {
    let mut seen = BTreeSet::new();
    for (label, error) in one_of_each_kind() {
        assert!(
            afd_core::error_code::REGISTRY.contains(&error.code()),
            "{label} reports {} which is not in the registry",
            error.code().as_str()
        );
        seen.insert(error.code());
    }
    assert_eq!(
        seen.len(),
        DISTINCT_CODES,
        "the kinds collapsed onto a different number of codes than {DISTINCT_CODES}, \
         which changes what a caller can discriminate on"
    );
}

/// The caller's sentence is present, and is not the internal message.
///
/// `detail()` is what a person is handed; the kind's `Display` is what the log
/// row carries. A detail that echoed the kind would leak the internal phrasing
/// into an API answer, and one that is blank tells the caller nothing at all.
#[test]
fn every_kind_hands_the_caller_a_sentence() {
    for (label, error) in one_of_each_kind() {
        let detail = error.detail();
        assert!(
            !detail.trim().is_empty(),
            "{label} hands the caller an empty sentence"
        );
        assert!(
            !detail.contains("[UZ-"),
            "{label} leaks a registry code into the caller's sentence: {detail}"
        );
    }
}

/// The caller-facing vocabulary is exactly these 3 sentences.
///
/// Pinned as a SET, not as a function of the code: kinds sharing a code may
/// still say different things here, and in this crate some do. What must not
/// drift is the vocabulary itself — a new sentence means a caller reading for
/// the old one stops matching, and two daemons answering one incident with
/// different prose read as two different bugs to whoever is holding the page.
#[test]
fn the_caller_facing_vocabulary_does_not_drift() {
    let sentences: BTreeSet<&str> = one_of_each_kind()
        .iter()
        .map(|(_, error)| error.detail())
        .collect();
    assert_eq!(
        sentences.len(),
        3,
        "the sentences handed to callers are now {sentences:?}"
    );
}

/// Each predicate answers for every kind, for exactly the measured set.
///
/// Pinned by count rather than by membership so that the test DISAGREES with a
/// change to a predicate instead of following it. A predicate that silently
/// widened would route a refusal to a retry, or a retry to a refusal.
#[test]
fn each_predicate_answers_for_exactly_the_kinds_it_owns() {
    let is_datastore_unavailable: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_datastore_unavailable())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_datastore_unavailable.len(),
        5,
        "`is_datastore_unavailable` now answers for {is_datastore_unavailable:?}, not 5 kinds"
    );

    let is_over_capacity: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_over_capacity())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_over_capacity.len(),
        3,
        "`is_over_capacity` now answers for {is_over_capacity:?}, not 3 kinds"
    );
}

/// Over capacity is always ALSO datastore-unavailable: both answer 503.
///
/// The narrowing is what an operator reads to tell a full queue from a dead
/// one. If it stopped implying the wide answer, a full queue would fall
/// outside the 503 handler entirely and surface as an unclassified 500.
#[test]
fn over_capacity_is_always_also_unavailable() {
    for (label, error) in one_of_each_kind() {
        if error.is_over_capacity() {
            assert!(
                error.is_datastore_unavailable(),
                "{label} is over capacity but denies being unavailable"
            );
        }
    }
}

/// The backtrace accessor answers for every kind without panicking.
#[test]
fn the_backtrace_accessor_answers_for_every_kind() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}

/// The exhausted-disk sample IS a Postgres disk-full error, not a stand-in.
///
/// It exists so the suites that grade this surface meet the one statement
/// failure an operator acts on differently from every other — the cure is
/// space, not a retry — and the only way to say that honestly is for the
/// sample to carry what the driver would carry: `53100`, the sentence the
/// server sends, and a kind the driver can classify. A fixture that answered
/// a bare string here would let the surface look covered while the branch an
/// operator needs was never built.
#[test]
fn the_exhausted_sample_carries_a_real_disk_full_database_error() {
    const DISK_FULL_SQLSTATE: &str = "53100";

    let (_label, exhausted) = one_of_each_kind()
        .into_iter()
        .find(|(label, _error)| *label == "exhausted")
        .expect("the sample declares an exhausted kind");

    let cause = exhausted.source().expect("a statement failure keeps its cause");
    let driver = cause
        .downcast_ref::<sqlx::Error>()
        .expect("the cause is the driver's own error, not a stringified copy");
    let sqlx::Error::Database(database) = driver else {
        panic!("an exhausted disk is a DATABASE error: {driver:?}");
    };

    assert_eq!(
        database.code().as_deref(),
        Some(DISK_FULL_SQLSTATE),
        "the SQLSTATE is what tells a disk-full apart from any other refused statement"
    );
    assert!(
        database.message().contains("No space left on device"),
        "the server's own sentence must survive: {}",
        database.message()
    );
    assert_eq!(
        database.kind(),
        sqlx::error::ErrorKind::Other,
        "sqlx classifies this as Other; a sample claiming a constraint violation \
         would route a caller to a retry that can never succeed"
    );
    // The chain is walkable all the way down as `std::error::Error`, which is
    // what an operator's log line renders.
    assert!(
        !database.as_error().to_string().is_empty(),
        "the driver error must render as itself"
    );
}
