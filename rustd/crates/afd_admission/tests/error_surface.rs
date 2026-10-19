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
//! pin: a lift swapped for a `to_string()` fails here instead of reaching an
//! operator as a broken chain.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, naming \
              which sample it was"
)]

use afd_core::test_util::error_surface::{Expect, grade, grade_sentences};
use std::error::Error as _;

use afd_admission::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
const WITH_SOURCE: usize = 7;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 3;

/// Everything every error surface owes, graded once by
/// `afd_core::test_util::error_surface`: registered codes, `[CODE] message`
/// rendering, no kind repeating its cause, and the measured counts above.
#[test]
fn the_error_surface_holds() {
    grade(
        &one_of_each_kind(),
        Expect {
            with_source: WITH_SOURCE,
            distinct_codes: DISTINCT_CODES,
        },
    );
}

/// The caller-facing vocabulary: present, uncoded, and exactly 3 sentences.
#[test]
fn the_caller_facing_sentences_hold() {
    let samples = one_of_each_kind();
    grade_sentences(
        samples
            .iter()
            .map(|(label, error)| (*label, error.detail())),
        3,
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

    let cause = exhausted
        .source()
        .expect("a statement failure keeps its cause");
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
