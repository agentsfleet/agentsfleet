//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a predicate that answers for the wrong kind, only
//! shows up while something else is already going wrong.
//!
//! # What this crate has that the others do not
//!
//! Every kind but one is a STARTUP failure: a declared metric family the census
//! contradicts. The daemon refuses to boot on these rather than serving blind.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a lift swapped for a `to_string()` fails here instead of reaching an
//! operator as a broken chain.

#![cfg(feature = "test-util")]

use afd_core::test_util::error_surface::{Expect, grade};

use afd_observability::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
///
/// Zero, and correctly so: the one lifted kind (`Census`) is
/// `#[error(transparent)]` over a `csv::Error`, whose own `source()` is the
/// default `None`. The chain ENDS at the reader's sentence, which already
/// carries the record, line and field. Nothing here can lose a link.
const WITH_SOURCE: usize = 0;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 2;

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

/// Each predicate answers for every kind, for exactly the measured set.
///
/// Pinned by count rather than by membership so that the test DISAGREES with a
/// change to a predicate instead of following it. A predicate that silently
/// widened would route a refusal to a retry, or a retry to a refusal.
#[test]
fn each_predicate_answers_for_exactly_the_kinds_it_owns() {
    let is_unreadable: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.is_unreadable())
        .map(|(label, _)| *label)
        .collect();
    assert_eq!(
        is_unreadable.len(),
        1,
        "`is_unreadable` answers true for {is_unreadable:?}. It owns the census \
         kind and exactly that one; a predicate that widened would route an \
         unreadable file to a retry, or a retry to a refusal"
    );
}
