//! Every accessor, code, rendering and source on the error type.
//!
//! These paths are what a human reads at three in the morning, and they are the
//! easiest to leave untested because the happy path never touches them. A
//! `Display` that panics, or an `is_*` that answers for two kinds at once, only
//! shows up while something else is already going wrong.
#![cfg(feature = "test-util")]
use std::error::Error as _;

use afd_redis::error::one_of_each_kind;

/// Every kind renders, carries its code, and says what went wrong.
#[test]
fn test_every_kind_renders_with_its_code() {
    let kinds = one_of_each_kind();
    assert!(kinds.len() >= 10, "a kind was added without a sample");

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
        assert!(
            error.source().is_some(),
            "{label} loses the chain a reader follows"
        );
    }
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
            error.is_unavailable(),
            error.is_command(),
            error.is_group_missing(),
            error.is_group_exists(),
            error.is_hub_closed(),
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

/// Every code is one this crate declared, spelled the way the registry spells it.
#[test]
fn test_every_code_is_registered() {
    for (label, error) in one_of_each_kind() {
        assert!(
            afd_core::error_code::REGISTRY.contains(&error.code()),
            "{label} reports {} which is not in the registry",
            error.code().as_str()
        );
    }
}

/// The backtrace accessor works whether or not one was captured.
#[test]
fn test_backtrace_is_always_answerable() {
    for (_label, error) in one_of_each_kind() {
        let _status = error.backtrace().status();
    }
}
