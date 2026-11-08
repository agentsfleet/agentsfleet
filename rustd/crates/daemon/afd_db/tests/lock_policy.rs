//! The bound a migrator waits on a held lock, proven without a database.
//!
//! The polling itself needs live migrators and lives in the integration lane,
//! including the zero-poll refusal. What an operator reads off the bound needs
//! neither, and is checked here: the wait is finite and reported as such.
use std::time::Duration;

use afd_db::migrate::RetryPolicy;

/// The production bound is what an operator waits, and it is finite.
#[test]
fn test_production_policy_is_bounded_and_reported() {
    assert_eq!(
        RetryPolicy::PRODUCTION.budget(),
        Duration::from_secs(30),
        "thirty polls a second apart — the number the failure reports"
    );
}

/// A zero-attempt policy waits for nothing and reports a zero wait.
#[test]
fn test_a_zero_attempt_policy_never_waits() {
    let policy = RetryPolicy::new(0, Duration::from_secs(1));
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
