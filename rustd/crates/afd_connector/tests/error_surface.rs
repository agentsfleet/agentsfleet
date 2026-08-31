//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them.
//!
//! # The distinction this crate must not lose
//!
//! A vendor that could not be REACHED and a vendor that answered and refused
//! are different failures with different next moves: the first is retryable
//! because nothing upstream saw the request, the second will answer the same
//! way forever and retrying it is outbound load with no end. They are separate
//! variants for that reason, and `is_retryable` is where the distinction pays.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_connector::error::{detail, one_of_each_kind};

/// The kinds a retry could plausibly fix, by the label the sample gives them.
const RETRYABLE: &[&str] = &["datastore", "vendor unreachable"];

#[test]
fn every_kind_renders_leading_with_its_code() {
    let kinds = one_of_each_kind();
    assert!(
        kinds.len() >= 10,
        "a kind was added to ErrorKind without a sample here"
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

/// Where there IS a source, it adds something rather than repeating us.
///
/// `RUST_ERROR_STANDARD` rule 4, as the standard states it — not "every variant
/// has a source", which the document explicitly calls wrong. `GrantUnreadable`
/// holds nothing and nothing caused it in the sense a chain walker means.
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
#[test]
fn a_wrapped_failure_is_still_reachable_through_the_chain() {
    let wrapped: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.source().is_some())
        .map(|(label, _)| *label)
        .collect();

    assert!(
        wrapped.len() >= 7,
        "only {wrapped:?} keep a source; a lift was replaced by a stringify"
    );
}

#[test]
fn every_code_is_one_the_registry_declares() {
    for (label, error) in one_of_each_kind() {
        assert!(
            afd_core::error_code::REGISTRY.contains(&error.code()),
            "{label} reports {} which is not in the registry",
            error.code().as_str()
        );
    }
}

#[test]
fn every_sentence_is_one_of_the_declared_details() {
    let declared = [
        detail::DATABASE_UNAVAILABLE,
        detail::DATABASE_ERROR,
        detail::OPERATION_FAILED,
        detail::VENDOR_UNREACHABLE,
        detail::EXCHANGE_FAILED,
    ];
    for (label, error) in one_of_each_kind() {
        assert!(
            declared.contains(&error.detail()),
            "{label} tells a caller `{}`, which is not a declared sentence",
            error.detail()
        );
    }
}

#[test]
fn only_the_kinds_a_retry_could_fix_are_retryable() {
    for (label, error) in one_of_each_kind() {
        assert_eq!(
            error.is_retryable(),
            RETRYABLE.contains(&label),
            "{label} decides the wrong way about whether the caller should try again"
        );
    }
}

/// A vendor that ANSWERED is not the same failure as one that never did.
#[test]
fn a_refusal_and_an_outage_are_never_the_same_answer() {
    let kinds = one_of_each_kind();
    let refused = kinds
        .iter()
        .find(|(label, _)| *label == "exchange refused")
        .expect("the sample declares a refusal");
    let unreachable = kinds
        .iter()
        .find(|(label, _)| *label == "vendor unreachable")
        .expect("the sample declares an outage");

    assert!(
        !refused.1.is_retryable(),
        "an authorization code the vendor already refused will be refused again"
    );
    assert!(
        unreachable.1.is_retryable(),
        "nothing upstream saw the request, so sending it again cannot double a grant"
    );
    assert_ne!(
        refused.1.code(),
        unreachable.1.code(),
        "the two send an operator to different places"
    );
}

/// Nothing an OAuth failure tells a caller may carry the vendor's own words.
///
/// The exchange handles a client secret, an authorization code and a token. A
/// sentence built from a vendor's response body is the one place those could
/// reach a caller, so the sentences are a closed set of constants and this is
/// what keeps them that way.
#[test]
fn no_sentence_carries_anything_from_the_exchange() {
    for (label, error) in one_of_each_kind() {
        let sentence = error.detail();
        for leak in ["400", "401", "token", "secret", "code=", "Bearer"] {
            assert!(
                !sentence.contains(leak),
                "{label} leaks `{leak}` into the caller's sentence: {sentence}"
            );
        }
    }
}

#[test]
fn the_backtrace_accessor_answers_for_every_kind() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}
