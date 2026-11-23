//! The startup settings build the wire's retry, and the defaults are the
//! existing provider behaviour. The retry's own suite is guffbolt-ai's
//! (`guffbolt_ai::wire::retry`).

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use guffbolt_ai::wire::Retry;

use super::settings::{RetryPolicy, RetrySettings};

#[test]
fn the_default_retry_policy_is_the_existing_provider_behavior() {
    assert_eq!(RetryPolicy::default(), RetryPolicy::DEFAULT);
    assert_eq!(RetryPolicy::DEFAULT.wire(), Retry::DEFAULT);
    let settings = garde::Unvalidated::new(RetrySettings::default())
        .validate()
        .unwrap();
    assert_eq!(RetryPolicy::new(&settings), RetryPolicy::DEFAULT);
}

#[test]
fn checked_settings_name_every_limit_of_the_wire_retry() {
    let settings = garde::Unvalidated::new(RetrySettings {
        max_attempts: 4,
        max_stream_open_attempts: 2,
        initial_backoff_ms: 5,
        max_retry_after_seconds: 7,
    })
    .validate()
    .unwrap();

    assert_eq!(
        RetryPolicy::new(&settings).wire(),
        Retry::DEFAULT
            .with_attempts(4)
            .with_openings(2)
            .with_backoff(Duration::from_millis(5))
            .with_wait_ceiling(Duration::from_secs(7))
    );
}
