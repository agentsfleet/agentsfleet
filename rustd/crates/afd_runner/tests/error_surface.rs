//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a predicate that answers for the wrong kind, only
//! shows up while something else is already going wrong.
//!
//! # What this crate has that the others do not
//!
//! Seven codes across the sampled kinds, because a caller here DOES discriminate — a
//! runner that vanished, one that is still leased and one refused for its host id
//! are three different things to answer with.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a lift swapped for a `to_string()` fails here instead of reaching an
//! operator as a broken chain.

#![cfg(feature = "test-util")]

use afd_core::test_util::error_surface::{Expect, grade, grade_sentences};

use afd_runner::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
const WITH_SOURCE: usize = 3;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 7;

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

/// The caller-facing vocabulary: present, uncoded, and exactly 7 sentences.
#[test]
fn the_caller_facing_sentences_hold() {
    let samples = one_of_each_kind();
    grade_sentences(
        samples
            .iter()
            .map(|(label, error)| (*label, error.detail())),
        7,
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
        2,
        "`is_datastore_unavailable` now answers for {is_datastore_unavailable:?}, not 2 kinds"
    );

    let is_rejected: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_rejected())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_rejected.len(),
        1,
        "`is_rejected` now answers for {is_rejected:?}, not 1 kinds"
    );

    let is_runner_vanished: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_runner_vanished())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_runner_vanished.len(),
        1,
        "`is_runner_vanished` now answers for {is_runner_vanished:?}, not 1 kinds"
    );
}
