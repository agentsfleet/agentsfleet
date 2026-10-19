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

use afd_core::test_util::error_surface::{Expect, grade};

use afd_connector::error::{detail, one_of_each_kind};

/// The kinds a retry could plausibly fix, by the label the sample gives them.
const RETRYABLE: &[&str] = &["datastore", "vendor unreachable"];

/// Everything every error surface owes, graded once by
/// `afd_core::test_util::error_surface`: registered codes, `[CODE] message`
/// rendering, no kind repeating its cause, and these measured counts. The
/// sample count is pinned exactly, so a kind added without a sample fails.
#[test]
fn the_error_surface_holds() {
    let samples = one_of_each_kind();
    assert_eq!(
        samples.len(),
        15,
        "a kind was added or dropped without its sample"
    );
    grade(
        &samples,
        Expect {
            with_source: 9,
            distinct_codes: 8,
        },
    );
}

#[test]
fn every_sentence_is_one_of_the_declared_details() {
    let declared = [
        detail::DATABASE_UNAVAILABLE,
        detail::DATABASE_ERROR,
        detail::OPERATION_FAILED,
        detail::VENDOR_UNREACHABLE,
        detail::EXCHANGE_FAILED,
        detail::INSTALLATION_OWNERSHIP,
        detail::INSTALLATION_LISTING_FAILED,
        detail::GRANT_STILL_REFERENCED,
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

/// A Disconnect the vault refused over a model entry names how many entries
/// hold it and answers the vault's conflict; no other kind claims a count.
///
/// The count is what lets the route answer 409 with `current_state` rather
/// than an operation failure a client would retry.
#[test]
fn only_a_referenced_refusal_carries_a_count_and_the_vault_conflict() {
    for (label, error) in one_of_each_kind() {
        if label == "vault referenced" {
            assert_eq!(error.referenced_by(), Some(2), "{error}");
            assert_eq!(
                error.code(),
                afd_core::error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES,
                "{error}"
            );
            assert_eq!(error.detail(), detail::GRANT_STILL_REFERENCED);
            assert!(
                !error.is_retryable(),
                "the entry must go before a retry can pass"
            );
        } else {
            assert_eq!(
                error.referenced_by(),
                None,
                "{label} claims a count it has none of"
            );
        }
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
