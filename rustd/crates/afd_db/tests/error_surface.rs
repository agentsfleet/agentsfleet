//! Every accessor, code, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or an `is_*` that answers for two kinds at once, only
//! shows up while something else is already going wrong.
#![cfg(feature = "test-util")]

use afd_core::test_util::error_surface::{Expect, grade};

use afd_db::error::one_of_each_kind;

/// Everything every error surface owes, graded once by
/// `afd_core::test_util::error_surface`: registered codes, `[CODE] message`
/// rendering, no kind repeating its cause, and these measured counts. The
/// sample count is pinned exactly, so a kind added without a sample fails.
#[test]
fn the_error_surface_holds() {
    let samples = one_of_each_kind();
    assert_eq!(
        samples.len(),
        14,
        "a kind was added or dropped without its sample"
    );
    grade(
        &samples,
        Expect {
            with_source: 5,
            distinct_codes: 3,
        },
    );
}

/// The accessors partition the kinds: exactly one answers for each.
///
/// This is the property that makes them useful. Two accessors answering for one
/// kind means a caller that matches on the first never sees the second, and a
/// kind no accessor claims is one nobody can handle at all.
#[test]
fn test_the_accessors_partition_the_kinds() {
    for (label, error) in one_of_each_kind() {
        let answers = [
            error.is_config(),
            error.is_pool_capacity(),
            error.is_datastore_unavailable(),
            error.is_query(),
            error.is_migration_failed(),
            error.is_migration_refused(),
        ];
        let claimed = answers.iter().filter(|answer| **answer).count();
        assert_eq!(claimed, 1, "{label} is claimed by {claimed} accessors");
    }
}

/// A stall is a refinement of "unavailable", never a partition of its own.
///
/// Every stall answers `is_datastore_unavailable`, because the request had no
/// datastore either way and every crate downstream turns on that question to
/// pick a 503. What the finer accessor adds is the one thing a caller can act
/// on — `Db::acquire` retries a stall and nothing else — and the census in the
/// message is what an operator acts on.
#[test]
fn test_a_stall_is_unavailable_and_carries_its_census() {
    for (label, error) in one_of_each_kind() {
        if !error.is_acquire_stalled() {
            continue;
        }
        assert!(
            error.is_datastore_unavailable(),
            "{label} is not unavailable"
        );
        assert!(!error.is_pool_capacity(), "{label} claims capacity");
        let rendered = error.to_string();
        assert!(
            rendered.contains(" of ") && rendered.contains("could not open another"),
            "{label} hides the census an operator needs: {rendered}"
        );
    }
    assert!(
        one_of_each_kind()
            .iter()
            .any(|(_label, error)| error.is_acquire_stalled()),
        "the sampler carries no stall to prove any of this against"
    );
}

/// Capacity and unreachable stay apart, because the operator's next move does.
#[test]
fn test_capacity_and_outage_are_never_the_same_answer() {
    for (label, error) in one_of_each_kind() {
        assert!(
            !(error.is_pool_capacity() && error.is_datastore_unavailable()),
            "{label} answers both"
        );
    }
}
