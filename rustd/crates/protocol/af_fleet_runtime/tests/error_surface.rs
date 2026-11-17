//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or a predicate that answers for the wrong kind, only
//! shows up while something else is already going wrong.
//!
//! # What this crate has that the others do not
//!
//! Every kind here reports ONE code, `UZ-AGT-008` — an authored document
//! was rejected, and a caller cannot act on which rule it broke. The structure
//! an author needs is in the message, which is why this suite grades rendering
//! and chains rather than a code partition.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a lift swapped for a `to_string()` fails here instead of reaching an
//! operator as a broken chain.

#![cfg(feature = "test-util")]

use af_core::test_util::error_surface::{Expect, grade};

use af_fleet_runtime::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
///
/// The two lifted kinds (`InvalidFieldType`, `OutOfBounds`) each keep their
/// cause under a message of their own, so they report one. Every other kind
/// here is a leaf that nothing caused.
const WITH_SOURCE: usize = 2;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 1;

/// Everything every error surface owes, graded once by
/// `af_core::test_util::error_surface`: registered codes, `[CODE] message`
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

/// Every kind names the setting it concerns in this crate's own spelling, or
/// none where the failure is the document's shape: a log may carry the answer,
/// so authored text such as `Not Kebab` or `not-a-key` must never be it.
#[test]
fn every_kind_names_its_setting_in_the_crates_own_spelling() {
    let expected = [
        ("missing required field", Some("name")),
        ("invalid definition", None),
        ("runtime key outside block", Some("gates")),
        ("unsupported key", Some("gates")),
        ("unknown runtime key", Some("x-agentsfleet")),
        ("runtime block required", Some("x-agentsfleet")),
        ("invalid name", Some("name")),
        ("invalid credential ref", Some("credentials")),
        ("invalid budget", Some("budget.daily_nanos")),
        ("invalid threshold", Some("threshold_count")),
        ("invalid trigger set", Some("triggers")),
        ("unverifiable webhook source", Some("triggers")),
        ("invalid signature config", Some("triggers")),
        ("invalid repository binding", Some("repositories")),
        ("invalid field type", None),
        ("out of bounds", None),
    ];
    let kinds = one_of_each_kind();
    assert_eq!(
        kinds.len(),
        expected.len(),
        "a new kind needs its expected setting here"
    );
    for ((label, error), (expected_label, field)) in kinds.iter().zip(expected) {
        assert_eq!(*label, expected_label);
        assert_eq!(error.field(), field, "{label}");
    }
}
