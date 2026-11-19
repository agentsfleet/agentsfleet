//! History cleanup budgets bound each statement and each table's drain.

use garde::Validate;
use serde::Deserialize;

/// Retains settled history longer than the protocol's maximum run runtime.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct RetentionSettings {
    /// Seconds between cycles after a clear pass.
    #[garde(range(min = 1, max = 86_400))]
    pub idle_interval_seconds: u64,
    /// Seconds between cycles while a backlog remains.
    #[garde(range(min = 1, max = self.idle_interval_seconds))]
    pub backlog_interval_seconds: u64,
    /// Whole days retained after settlement; always exceeds the twelve-hour run ceiling.
    #[garde(range(min = 1, max = 3650))]
    pub keep_days: u32,
    /// Maximum affected rows in each statement.
    #[garde(range(min = 1, max = 10_000))]
    pub batch_size: i64,
    /// Maximum consecutive statements for each table in one cycle.
    #[garde(range(min = 1, max = 64))]
    pub max_batches: usize,
}

impl RetentionSettings {
    pub(super) const DEFAULT: Self = Self {
        idle_interval_seconds: super::IDLE_INTERVAL.as_secs(),
        backlog_interval_seconds: super::SATURATED_INTERVAL.as_secs(),
        keep_days: super::RETENTION_DAYS,
        batch_size: super::BATCH_LIMIT,
        max_batches: super::MAX_BATCHES,
    };
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}
