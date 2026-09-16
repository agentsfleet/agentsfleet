//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a person reads at three in the morning, and they are
//! the easiest to leave untested because the happy path never touches them.
//!
//! # The distinction this crate must not lose
//!
//! A database that would not commit is a REFUSAL a producer must retry
//! through: nothing was accepted. A queue that would not take an append is
//! not, because the row is already committed — the caller was answered and
//! the sweeper owes it an entry. Both reach the HTTP edge only through the
//! replay path's own reporting, and `is_datastore_unavailable` is what
//! decides 503 from 500 when they do. Capacity — a spent budget, a full
//! queue, a Postgres out of disk — is a 503 to the caller and its own class
//! to the operator, and both halves are asserted.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::collections::BTreeSet;
use std::error::Error as _;

use afd_core::error::{
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_OPERATION_FAILED,
};
use afd_core::error_code;

use super::one_of_each_kind;

/// The labels the sample gives the failures a retry could plausibly fix.
///
/// Named here rather than derived from the errors themselves: a test that
/// asked the type which kinds are outages and then asserted the answer would
/// agree with any answer. This is the list a person maintains, so a variant
/// that changes sides has to be moved by hand.
const UNREACHABLE: &[&str] = &["datastore", "queue unreachable"];

/// The labels the sample gives capacity, which a caller retries like an
/// outage and an operator reads as a different incident.
const CAPACITY: &[&str] = &["over budget", "exhausted", "queue full"];

/// Everything a caller retries: the datastore that will not answer and the
/// datastore that answers "not now".
fn is_outage(label: &str) -> bool {
    UNREACHABLE.contains(&label) || CAPACITY.contains(&label)
}

#[test]
fn every_kind_renders_leading_with_its_code() {
    let kinds = one_of_each_kind();
    assert!(
        kinds.len() >= 6,
        "a kind was added to ErrorKind without a sample beside it"
    );

    for (label, error) in &kinds {
        let rendered = error.to_string();
        assert!(
            rendered.starts_with(&format!("[{}]", error.code().as_str())),
            "{label} does not lead with its code: {rendered}"
        );
        assert!(
            rendered.len() > error.code().as_str().len() + 3,
            "{label} renders its code and nothing else"
        );
    }
}

#[test]
fn no_error_reports_itself_as_its_own_cause() {
    // `RUST_ERROR_STANDARD` rule 4 as the standard states it — not "every
    // variant has a source", which that document explicitly calls wrong.
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

#[test]
fn only_a_datastore_that_could_not_be_reached_reports_an_outage() {
    // The 503-versus-500 decision, and the reason `Queue` is sampled twice. A
    // queue that is GONE is the outage a caller retries against; a queue that
    // answered and refused will answer the same way forever.
    for (label, error) in one_of_each_kind() {
        let expected = is_outage(label);
        assert_eq!(
            error.is_datastore_unavailable(),
            expected,
            "{label} is on the wrong side of the retry decision"
        );
    }
}

#[test]
fn an_outage_answers_the_unavailable_code_and_everything_else_does_not() {
    for (label, error) in one_of_each_kind() {
        if is_outage(label) {
            assert_eq!(
                error.code(),
                error_code::INTERNAL_DB_UNAVAILABLE,
                "{label} is an outage and must answer the code a retry reads"
            );
            assert_eq!(error.detail(), DETAIL_DATABASE_UNAVAILABLE);
        } else {
            assert_ne!(
                error.code(),
                error_code::INTERNAL_DB_UNAVAILABLE,
                "{label} is not an outage and must not invite a retry"
            );
        }
    }
}

#[test]
fn a_statement_that_would_not_run_names_its_operation_to_the_operator_only() {
    // Both halves matter and they point opposite ways. The operator needs to
    // know which statement to go and look at, so the rendering carries it;
    // the caller must not, so the sentence does not.
    let (_label, query) = one_of_each_kind()
        .into_iter()
        .find(|(label, _error)| *label == "query")
        .expect("the sample carries a statement failure");
    assert!(
        query.to_string().contains("admitting an event"),
        "the operator is not told which statement failed: {query}"
    );
    assert_eq!(query.detail(), DETAIL_DATABASE_ERROR);
    assert!(
        !query.detail().contains("admitting"),
        "the operation leaked into the caller's sentence"
    );
    assert_eq!(query.code(), error_code::INTERNAL_DB_QUERY);
}

#[test]
fn no_sentence_names_which_internal_failure_it_was() {
    // Three internal failures share one sentence on purpose. Naming which of
    // them it was would tell whoever provoked it something about this
    // deployment's state, and an ingress caller is exactly who must not learn
    // it — the webhook endpoint is public until the signature passes.
    let opaque: Vec<&'static str> = one_of_each_kind()
        .into_iter()
        .filter(|(label, _error)| ["queue answered", "entropy", "identifier"].contains(label))
        .map(|(_label, error)| error.detail())
        .collect();

    assert_eq!(
        opaque.len(),
        3,
        "the sample lost one of the opaque failures"
    );
    let distinct: BTreeSet<&&str> = opaque.iter().collect();
    assert_eq!(
        distinct.len(),
        1,
        "three internal failures must be indistinguishable to a caller: {opaque:?}"
    );
    assert_eq!(
        opaque.first().copied(),
        Some(DETAIL_OPERATION_FAILED),
        "and the one sentence they share is the generic one"
    );
}

#[test]
fn every_sentence_is_one_the_workspace_declares() {
    // No crate-local spelling of a sentence ten planes answer. A copy here
    // would be a tenth place one string can drift, and a client watching for
    // the pair would see two different sentences for one condition.
    let declared = [
        DETAIL_DATABASE_UNAVAILABLE,
        DETAIL_DATABASE_ERROR,
        DETAIL_OPERATION_FAILED,
    ];
    for (label, error) in one_of_each_kind() {
        assert!(
            declared.contains(&error.detail()),
            "{label} answers a sentence this workspace does not declare: {}",
            error.detail()
        );
    }
}

#[test]
fn capacity_is_an_outage_to_the_caller_and_its_own_class_to_the_operator() {
    // Every capacity refusal invites the retry an outage does, and nothing
    // else is allowed to call itself capacity: a queue that is merely gone is
    // an outage, not a full one.
    for (label, error) in one_of_each_kind() {
        let expected = CAPACITY.contains(&label);
        assert_eq!(
            error.is_over_capacity(),
            expected,
            "{label} is on the wrong side of the capacity class"
        );
        if expected {
            assert!(
                error.is_datastore_unavailable(),
                "{label} is capacity and must still invite a retry"
            );
        }
    }
}
