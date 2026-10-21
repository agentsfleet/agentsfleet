//! Every accessor, code, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or an `is_*` that answers for two kinds at once, only
//! shows up while something else is already going wrong.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]
use afd_core::test_util::error_surface::{Expect, grade};
use std::error::Error as _;

use afd_dragonfly::error::one_of_each_kind;

/// Everything every error surface owes, graded once by
/// `afd_core::test_util::error_surface`: registered codes, `[CODE] message`
/// rendering, no kind repeating its cause, and these measured counts. The
/// sample count is pinned exactly, so a kind added without a sample fails.
#[test]
fn the_error_surface_holds() {
    let samples = one_of_each_kind();
    assert_eq!(
        samples.len(),
        19,
        "a kind was added or dropped without its sample"
    );
    grade(
        &samples,
        Expect {
            with_source: 6,
            distinct_codes: 3,
        },
    );
}

/// During redial the driver can have no connections before it reports an I/O error.
#[test]
fn a_cluster_without_connections_is_unavailable_and_keeps_the_driver_cause() {
    let (_label, failure) = one_of_each_kind()
        .into_iter()
        .find(|(label, _)| *label == "unreachable")
        .expect("the public error samples include an unreachable cluster");
    assert!(failure.is_unavailable());
    assert!(!failure.is_command());
    assert_eq!(
        failure.code(),
        afd_core::error_code::STARTUP_DRAGONFLY_CONNECT
    );
    let source = failure
        .source()
        .and_then(|source| source.downcast_ref::<Box<redis::RedisError>>())
        .expect("the typed driver cause survives classification");
    assert_eq!(source.kind(), redis::ErrorKind::ClusterConnectionNotFound);
}

/// The accessors partition the kinds: exactly one answers for each.
///
/// This is the property that makes them useful. Two accessors answering for one
/// kind means a caller that matches on the first never sees the second, and a
/// kind no accessor claims is one nobody can handle at all.
///
/// `is_certificate_rejected` is deliberately NOT in the list below. It narrows
/// `is_unavailable` rather than standing beside it — a rejected certificate is
/// an unusable datastore that happens to say which of two places to look — so
/// adding it here would claim its kind twice and fail this test. That is the
/// test working, not a gap in it.
#[test]
fn test_the_accessors_partition_the_kinds() {
    for (label, error) in one_of_each_kind() {
        let answers = [
            error.is_config(),
            error.is_unavailable(),
            error.is_command(),
            error.is_group_missing(),
            error.is_group_exists(),
            error.is_hub_closed(),
            error.is_full(),
            error.is_unsafe_eviction(),
            error.is_unsuitable_datastore(),
        ];
        let claimed = answers.iter().filter(|answer| **answer).count();
        assert_eq!(claimed, 1, "{label} is claimed by {claimed} accessors");
    }
}

/// An outage and a refused command stay apart: one is the datastore, one is us.
#[test]
fn test_availability_and_command_failure_are_never_one_answer() {
    for (label, error) in one_of_each_kind() {
        assert!(
            !(error.is_unavailable() && error.is_command()),
            "{label} answers both"
        );
    }
}
