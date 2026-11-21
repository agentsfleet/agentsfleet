//! New objectives snapshot these allowances; existing tasks retain their saved limits.

use af_wire::task::TaskLimits;
use garde::Validate;
use serde::Deserialize;

/// Initial cumulative allowances, subject to the fleet's smaller available budget.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct TaskDefaults {
    /// Total paid model allowance in whole nanodollars.
    #[garde(range(min = 1))]
    pub model_budget_nanodollars: i64,
    /// Lifetime including paused work and time waiting for a person.
    #[garde(range(min = 1, max = 31_536_000))]
    pub wall_seconds: u32,
    /// Accumulated active time across every replacement run.
    #[garde(range(min = 1, max = self.wall_seconds))]
    pub running_seconds: u32,
    /// Total distinct issued runs, including replacements.
    #[garde(range(min = 1, max = 65_535))]
    pub max_runs: u32,
    /// Total physical repair publication attempts.
    #[garde(range(min = 1, max = 65_535))]
    pub max_publication_attempts: u32,
    /// Total physical decision calls, including eligible paid retries.
    #[garde(range(min = 1, max = 65_535))]
    pub max_decision_calls: u32,
    /// Total dependency fallback reads, separate from signed evidence.
    #[garde(range(min = 1, max = 65_535))]
    pub max_fallback_reads: u32,
}

impl Default for TaskDefaults {
    fn default() -> Self {
        let limits = TaskLimits::default();
        Self {
            model_budget_nanodollars: limits.model_nanos,
            wall_seconds: limits.wall_seconds,
            running_seconds: limits.running_seconds,
            max_runs: limits.runs,
            max_publication_attempts: limits.publications,
            max_decision_calls: limits.decision_calls,
            max_fallback_reads: limits.fallback_reads,
        }
    }
}

impl TaskDefaults {
    pub(super) const fn snapshot(self) -> TaskLimits {
        TaskLimits {
            model_nanos: self.model_budget_nanodollars,
            wall_seconds: self.wall_seconds,
            running_seconds: self.running_seconds,
            runs: self.max_runs,
            publications: self.max_publication_attempts,
            decision_calls: self.max_decision_calls,
            fallback_reads: self.max_fallback_reads,
        }
    }
}
