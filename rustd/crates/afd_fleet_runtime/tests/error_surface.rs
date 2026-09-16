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
//! was rejected, and a caller cannot act on which rule it broke. `class()` is the
//! discriminator instead, which is why this suite grades the class partition and
//! not the code partition.
//!
//! Every count below was MEASURED against the sample set, not chosen. Each is a
//! pin: a kind added without a sample, or a lift swapped for a `to_string()`,
//! fails here instead of reaching an operator as a broken chain.

#![cfg(feature = "test-util")]

use std::collections::BTreeSet;
use std::error::Error as _;

use afd_fleet_runtime::error::one_of_each_kind;

/// How many kinds the sample declares.
const SAMPLES: usize = 17;
/// How many carry a `source()` — the chain an operator follows to the cause.
///
/// The three lifted kinds (`InvalidFieldType`, `OutOfBounds`,
/// `FrontmatterUnreadable`) each keep their cause under a message of their own,
/// so they report one. Every other kind here is a leaf that nothing caused.
const WITH_SOURCE: usize = 3;
/// How many distinct registry codes the kinds report between them.
const DISTINCT_CODES: usize = 1;

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

/// The class partitions the kinds where the code cannot.
///
/// All fourteen kinds report `UZ-AGT-008`, so `class()` is the only thing a
/// caller can discriminate on. A class that collapsed would take that away
/// silently — the code would still be right and the answer still useless.
#[test]
fn the_class_is_what_a_caller_discriminates_on() {
    let classes: BTreeSet<String> = one_of_each_kind()
        .iter()
        .map(|(_, error)| format!("{:?}", error.class()))
        .collect();
    assert_eq!(
        classes.len(),
        5,
        "the kinds collapsed onto {classes:?}; the code cannot tell them apart, \
         so the class is the whole discriminator and a merge is a lost answer"
    );
}

/// The backtrace accessor answers for every kind without panicking.
#[test]
fn the_backtrace_accessor_answers_for_every_kind() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}
