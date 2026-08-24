//! Dimension 2.2 (decision half) — the bound, proven without a database.
//!
//! The contention proof needs two live migrators and lives in the integration
//! lane. What the bound *decides* needs neither, and this is where the property
//! that matters is checked: the loop terminates. A policy that never returns
//! `Exhausted` is a `pg_advisory_lock` with extra steps — a deploy that hangs
//! until the deploy machine gives up, which is the failure the bound exists to
//! prevent.
use std::time::Duration;

use afd_db::migrate::{Attempt, RetryPolicy};

/// The production bound is what an operator waits, and it is finite.
#[test]
fn test_production_policy_is_bounded_and_reported() {
    assert_eq!(
        RetryPolicy::PRODUCTION.budget(),
        Duration::from_secs(30),
        "thirty polls a second apart — the number the failure reports"
    );
}

/// Holding the lock ends the loop immediately, whatever the attempt number.
#[test]
fn test_an_acquired_lock_stops_the_loop() {
    let policy = RetryPolicy::new(3, Duration::from_millis(1));
    for attempt in 1..=5 {
        assert_eq!(
            afd_db::migrate::lock::classify(true, attempt, policy),
            Attempt::Acquired,
            "attempt {attempt}"
        );
    }
}

/// Contention retries until the bound, then stops. The last attempt is
/// `Exhausted`, not `Retry` — an off-by-one here is an extra poll on every
/// contended deploy, and at the boundary it is an infinite loop.
#[test]
fn test_contention_retries_until_the_bound_then_stops() {
    let policy = RetryPolicy::new(3, Duration::from_millis(1));
    assert_eq!(
        afd_db::migrate::lock::classify(false, 1, policy),
        Attempt::Retry
    );
    assert_eq!(
        afd_db::migrate::lock::classify(false, 2, policy),
        Attempt::Retry
    );
    assert_eq!(
        afd_db::migrate::lock::classify(false, 3, policy),
        Attempt::Exhausted,
        "the final attempt must not ask for another"
    );
    assert_eq!(
        afd_db::migrate::lock::classify(false, 4, policy),
        Attempt::Exhausted,
        "past the bound stays exhausted"
    );
}

/// A zero-attempt policy is exhausted at once rather than looping forever.
#[test]
fn test_a_zero_attempt_policy_never_waits() {
    let policy = RetryPolicy::new(0, Duration::from_secs(1));
    assert_eq!(
        afd_db::migrate::lock::classify(false, 1, policy),
        Attempt::Exhausted
    );
    assert_eq!(policy.budget(), Duration::ZERO);
}

/// The reported wait cannot overflow into a nonsense number, which is what an
/// operator would otherwise read off a misconfigured policy.
#[test]
fn test_the_reported_budget_saturates() {
    let policy = RetryPolicy::new(u32::MAX, Duration::from_secs(u64::MAX / 2));
    assert!(
        policy.budget() > Duration::from_secs(1),
        "must not wrap to 0"
    );
}
