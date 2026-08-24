//! The real environment source, which every other test deliberately avoids.
//!
//! `MapEnv` is what the config tests drive, precisely so a parallel suite never
//! races on the process environment — `std::env::set_var` is `unsafe` in edition
//! 2024 because that race is undefined behaviour, not flakiness. The
//! consequence is that [`ProcessEnv`], the implementation the DAEMON actually
//! runs, is the one nothing exercises. These tests read the process environment
//! without ever writing it, which is safe from any number of threads.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::{EnvSource, ProcessEnv};

/// A name no process exports. Long and namespaced so it cannot collide with a
/// developer's shell by accident.
const ABSENT: &str = "AFD_CORE_ENV_TEST_VARIABLE_THAT_IS_NEVER_SET";

/// A variable the process really has is read back through the trait.
///
/// `PATH` is the one name POSIX guarantees a test process inherits, which is
/// what makes this an assertion rather than a coin flip.
#[test]
fn test_the_process_environment_is_read_through_the_trait() {
    let path = ProcessEnv
        .get("PATH")
        .expect("a test process always inherits PATH");
    assert!(!path.is_empty(), "PATH came back empty");
    assert_eq!(
        Some(path),
        std::env::var("PATH").ok(),
        "the source must report what the process actually has"
    );
}

/// An unset variable is `None`, not an empty string.
///
/// The distinction is load-bearing one layer up: `crate::config` treats blank
/// as unset, and it can only make that decision if this layer keeps the two
/// apart rather than flattening them here.
#[test]
fn test_an_unset_variable_is_absent_rather_than_blank() {
    assert_eq!(ProcessEnv.get(ABSENT), None);
}

/// Reading is not writing: the source leaves the environment as it found it.
#[test]
fn test_reading_does_not_mutate_the_environment() {
    let before = std::env::vars().count();
    let _absent = ProcessEnv.get(ABSENT);
    let _present = ProcessEnv.get("PATH");
    assert_eq!(
        std::env::vars().count(),
        before,
        "a read must not add or remove a variable"
    );
}
