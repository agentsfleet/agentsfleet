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

use afd_core::test_util::error_surface::{Expect, grade};

use afd_cron::error::{detail, one_of_each_kind};

/// The kinds a retry could plausibly fix, by the label the sample gives them.
///
/// Named here rather than asked of the type, so that the test disagrees with a
/// change to `is_retryable` instead of following it.
const RETRYABLE: &[&str] = &["datastore", "upstream unreachable"];

/// The kinds a refused schedule fire is raised as, by sample label.
const FIRE_REFUSALS: &[&str] = &[
    "keys missing",
    "token malformed",
    "signature invalid",
    "body mismatch",
];

/// The word every kind that is not a refused fire reads under.
const OPERATIONAL_REASON: &str = "operational_fault";

/// Everything every error surface owes, graded once by
/// `afd_core::test_util::error_surface`: registered codes, `[CODE] message`
/// rendering, no kind repeating its cause, and these measured counts. The
/// sample count is pinned exactly, so a kind added without a sample fails.
#[test]
fn the_error_surface_holds() {
    let samples = one_of_each_kind();
    assert_eq!(
        samples.len(),
        16,
        "a kind was added or dropped without its sample"
    );
    grade(
        &samples,
        Expect {
            with_source: 6,
            distinct_codes: 5,
        },
    );
}

#[test]
fn every_sentence_is_one_of_the_declared_details() {
    let declared = [
        detail::DATABASE_UNAVAILABLE,
        detail::DATABASE_ERROR,
        detail::OPERATION_FAILED,
        detail::UPSTREAM_UNAVAILABLE,
        detail::RETIRE_HELD,
        detail::FIRE_UNVERIFIED,
        detail::DESTINATION_UNUSABLE,
        detail::DESTINATION_NOT_PLAIN_ORIGIN,
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

/// Only a refused fire reads under a refusal's own word; every other kind is
/// this daemon failing, and they share one so the answer stays total.
#[test]
fn only_a_refused_fire_reads_under_a_refusal_word() {
    for (label, error) in one_of_each_kind() {
        assert_eq!(
            error.refusal_word() == OPERATIONAL_REASON,
            !FIRE_REFUSALS.contains(&label),
            "{label} reads as `{}`",
            error.refusal_word()
        );
    }
}
