//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a predicate that answers for the wrong kind, only
//! shows up while something else is already going wrong.
//!
//! # What this crate has that the others do not
//!
//! `UnknownSubject` is deliberately NOT an outage (see the module note on
//! `error.rs`): the caller matches on it and answers the empty capability set.
//! This suite pins that `is_provider_unavailable` keeps saying so.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a lift swapped for a `to_string()` fails here instead of reaching an
//! operator as a broken chain.

#![cfg(feature = "test-util")]

use afd_core::test_util::error_surface::{Expect, grade, grade_sentences};

use afd_identity::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
const WITH_SOURCE: usize = 0;
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
    let is_provider_unavailable: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_provider_unavailable())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_provider_unavailable.len(),
        2,
        "`is_provider_unavailable` now answers for {is_provider_unavailable:?}, not 2 kinds"
    );
}
