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

use afd_core::test_util::error_surface::{Expect, grade};

use afd_fleet_runtime::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
///
/// The two lifted kinds (`InvalidFieldType`, `OutOfBounds`) each keep their
/// cause under a message of their own, so they report one. Every other kind
/// here is a leaf that nothing caused.
const WITH_SOURCE: usize = 2;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 1;

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
