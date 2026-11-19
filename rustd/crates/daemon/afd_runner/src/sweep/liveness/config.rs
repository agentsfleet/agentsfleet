//! Validated cadence and batch size for silent-host maintenance.

use garde::Validate;
use serde::Deserialize;

/// Operational limits; lease expiry and the offline threshold have separate clocks.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct LivenessSettings {
    /// Seconds between bounded due-runner passes.
    #[garde(range(min = 1, max = 3600))]
    pub interval_seconds: u64,
    /// Maximum due runners selected by one pass.
    #[garde(range(min = 1, max = 10_000))]
    pub batch_size: i64,
}

impl LivenessSettings {
    pub(super) const DEFAULT: Self = Self {
        interval_seconds: super::HEARTBEAT_INTERVAL.as_secs(),
        batch_size: super::BATCH_LIMIT,
    };
}

impl Default for LivenessSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}
