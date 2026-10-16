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

use std::collections::BTreeSet;
use std::error::Error as _;

use afd_fleet_runtime::error::one_of_each_kind;

/// How many carry a `source()` — the chain an operator follows to the cause.
///
/// The two lifted kinds (`InvalidFieldType`, `OutOfBounds`) each keep their
/// cause under a message of their own, so they report one. Every other kind
/// here is a leaf that nothing caused.
const WITH_SOURCE: usize = 2;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 1;

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

/// The backtrace accessor answers for every kind without panicking.
#[test]
fn the_backtrace_accessor_answers_for_every_kind() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}
