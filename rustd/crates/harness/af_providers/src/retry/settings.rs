//! Independent limits for physical sends and eligible stream reopenings.

use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

/// Total attempts include the first send or opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct RetrySettings {
    /// HTTP sends within one opening, before any response body is consumed.
    #[garde(range(min = 1, max = 10))]
    pub max_attempts: usize,
    /// Reopenings are allowed only before visible output or known spend.
    #[garde(range(min = 1, max = 10))]
    pub max_stream_open_attempts: usize,
    /// Initial exponential delay, with jitter.
    #[garde(range(min = 1, max = 60_000))]
    pub initial_backoff_ms: u64,
    /// A larger Retry-After ends the attempt instead of sleeping.
    #[garde(range(min = 1, max = 300))]
    pub max_retry_after_seconds: u64,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            max_stream_open_attempts: 3,
            initial_backoff_ms: 1000,
            max_retry_after_seconds: 60,
        }
    }
}

/// Checked settings shared by all lease providers on one runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub(super) attempts: usize,
    pub(crate) openings: usize,
    pub(super) backoff: Duration,
    pub(super) wait_ceiling: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl RetryPolicy {
    /// Preserves the existing three sends and three eligible openings.
    pub const DEFAULT: Self = Self {
        attempts: 3,
        openings: 3,
        backoff: Duration::from_secs(1),
        wait_ceiling: Duration::from_secs(60),
    };

    /// Constructs the policy after startup validation.
    #[must_use]
    pub fn new(settings: &garde::Valid<RetrySettings>) -> Self {
        Self {
            attempts: settings.max_attempts,
            openings: settings.max_stream_open_attempts,
            backoff: Duration::from_millis(settings.initial_backoff_ms),
            wait_ceiling: Duration::from_secs(settings.max_retry_after_seconds),
        }
    }
}
