//! Runner-local retry and cadence settings; daemon assignments still own work slots.

use backon::{BackoffBuilder as _, ExponentialBuilder};
use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

/// Startup knobs bounded by the existing lease and physical cleanup windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeSettings {
    /// Total bounded daemon sends, including the first send.
    #[garde(range(min = 1, max = 10))]
    pub daemon_max_attempts: usize,
    /// Initial jittered pause for bounded calls and connection recovery.
    #[garde(range(min = 1, max = self.daemon_max_backoff_ms))]
    pub daemon_initial_backoff_ms: u64,
    /// Exponential ceiling before jitter; reconnect and reports are unbounded in count.
    #[garde(range(min = 1, max = 30_000))]
    pub daemon_max_backoff_ms: u64,
    /// Batched host renewal cadence; cannot exceed the existing safe cadence.
    #[garde(range(min = 100, max = 5000))]
    pub renewal_interval_ms: u64,
    /// Authoring renewal cadence; absolute accepted deadlines still win.
    #[garde(range(min = 100, max = 10_000))]
    pub authoring_renewal_interval_ms: u64,
    /// Pause after a retryable authoring renewal failure.
    #[garde(range(min = 1, max = 250))]
    pub authoring_renewal_retry_ms: u64,
    /// Required file restores queue; optional captures skip when admission is full.
    #[garde(range(min = 1, max = 16))]
    pub max_concurrent_file_operations: usize,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        RuntimePolicy::DEFAULT.settings
    }
}

/// Copyable checked policy, injected before the runner connects to the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePolicy {
    settings: RuntimeSettings,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl RuntimePolicy {
    /// Existing runner behavior, including four retries after the initial send.
    pub const DEFAULT: Self = Self {
        settings: RuntimeSettings {
            daemon_max_attempts: 5,
            daemon_initial_backoff_ms: 250,
            daemon_max_backoff_ms: 30_000,
            renewal_interval_ms: 5000,
            authoring_renewal_interval_ms: 10_000,
            authoring_renewal_retry_ms: 250,
            max_concurrent_file_operations: 2,
        },
    };

    /// Accepts only startup settings proved within their safety bounds.
    #[must_use]
    pub fn new(settings: &garde::Valid<RuntimeSettings>) -> Self {
        Self {
            settings: **settings,
        }
    }

    /// Bounded sends, including the first.
    pub(crate) const fn attempts(self) -> usize {
        self.settings.daemon_max_attempts
    }

    /// Checked values for components sharing this runner's resources.
    #[must_use]
    pub const fn settings(self) -> RuntimeSettings {
        self.settings
    }

    pub(crate) fn backoff(self) -> ExponentialBuilder {
        ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(
                self.settings.daemon_initial_backoff_ms,
            ))
            .with_max_delay(Duration::from_millis(self.settings.daemon_max_backoff_ms))
            .with_jitter()
    }

    pub(crate) fn endless(self) -> backon::ExponentialBackoff {
        self.backoff().without_max_times().build()
    }
}
