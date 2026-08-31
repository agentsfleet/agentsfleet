//! Every accessor, code, sentence, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or an `is_retryable` that answers for the wrong kind,
//! only shows up while something else is already going wrong.
//!
//! # The one this crate has that the others do not
//!
//! `:sync` exists to retry, so `is_retryable` is not a convenience here — it is
//! the predicate that decides whether a failed schedule is picked up again or
//! left sitting. A kind that answered it wrongly would either strand a schedule
//! that a retry would have fixed, or retry forever against a vendor saying no.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_cron::error::{detail, one_of_each_kind};

/// The kinds a retry could plausibly fix, by the label the sample gives them.
///
/// Named here rather than asked of the type, so that the test disagrees with a
/// change to `is_retryable` instead of following it.
const RETRYABLE: &[&str] = &["datastore", "upstream unreachable"];

#[test]
fn every_kind_renders_leading_with_its_code() {
    let kinds = one_of_each_kind();
    assert!(
        kinds.len() >= 9,
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
/// Not every kind has one and that is not a defect: `UpstreamRefused` holds a
/// status and nothing caused it in the sense a chain walker means. What no
/// error may do is report itself as its own cause — the failure
/// `RUST_ERROR_STANDARD` rule 4 exists to prevent, which prints the same
/// sentence twice to any `{:#}` walker before reaching anything new.
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
/// The lifts are `From`, so nothing on these paths does
/// `map_err(|e| Mine(e.to_string()))` — which compiles, reads the same at the
/// call site, and destroys the chain an operator follows to the real cause.
#[test]
fn a_wrapped_failure_is_still_reachable_through_the_chain() {
    let wrapped: Vec<&str> = one_of_each_kind()
        .iter()
        .filter(|(_, error)| error.source().is_some())
        .map(|(label, _)| *label)
        .collect();

    assert!(
        wrapped.len() >= 6,
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
        detail::UPSTREAM_UNAVAILABLE,
    ];
    for (label, error) in one_of_each_kind() {
        assert!(
            declared.contains(&error.detail()),
            "{label} tells a caller `{}`, which is not a declared sentence — \
             two daemons answering one incident with different prose read as \
             two different bugs to whoever is holding the page",
            error.detail()
        );
    }
}

/// Retryable is exactly the two kinds, and `:sync` depends on it being exact.
#[test]
fn only_the_kinds_a_retry_could_fix_are_retryable() {
    for (label, error) in one_of_each_kind() {
        assert_eq!(
            error.is_retryable(),
            RETRYABLE.contains(&label),
            "{label} decides the wrong way about whether `:sync` should try again"
        );
    }
}

/// A vendor that ANSWERED is not the same failure as one that never did.
///
/// They are separate variants on purpose: nothing upstream has seen an
/// unreachable request, so re-sending it cannot double anything, while a
/// refusal means the call landed and retrying it unchanged has no reason to do
/// better. Collapsing them would make `:sync` hammer a vendor that already said
/// no.
#[test]
fn a_refusal_and_an_outage_are_never_the_same_answer() {
    let kinds = one_of_each_kind();
    let refused = kinds
        .iter()
        .find(|(label, _)| *label == "upstream refused")
        .expect("the sample declares a refusal");
    let unreachable = kinds
        .iter()
        .find(|(label, _)| *label == "upstream unreachable")
        .expect("the sample declares an outage");

    assert!(!refused.1.is_retryable(), "a vendor that said no said no");
    assert!(
        unreachable.1.is_retryable(),
        "nothing upstream saw the request, so sending it again cannot double anything"
    );
}

/// The vendor's status reaches the operator's row, never the caller's answer.
///
/// A person editing a schedule cannot act on "the scheduler answered 429". What
/// they are told is that the row is saved, the schedule is not yet live, and it
/// will be retried. The status lives in `last_error`, where an operator reads
/// it — so it must not appear in what the caller is handed.
#[test]
fn a_vendor_status_never_reaches_the_caller() {
    for (label, error) in one_of_each_kind() {
        assert!(
            !error.detail().contains("429") && !error.detail().contains("status"),
            "{label} leaks the vendor's answer into the caller's sentence: {}",
            error.detail()
        );
    }
}

#[test]
fn the_backtrace_accessor_answers_for_every_kind() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}
