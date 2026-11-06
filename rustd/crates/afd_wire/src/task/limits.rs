//! Cumulative allowances do not reset when another lease resumes the task.

use serde::{Deserialize, Serialize};

/// Approved initial limits; a smaller existing fleet or credit limit wins.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLimits {
    /// Total paid-model allowance, in whole nanodollars.
    #[serde(with = "crate::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = crate::nanos::Decimal))]
    pub model_nanos: i64,
    /// Wall-clock lifetime including paused and waiting time.
    pub wall_seconds: u32,
    /// Cumulative running time across all attempts.
    pub running_seconds: u32,
    /// Maximum distinct issued leases.
    pub leases: u32,
    /// Maximum physical repair publication attempts.
    pub publications: u32,
    /// Maximum physical decision calls, including retries.
    pub decision_calls: u32,
    /// Maximum fallback dependency reads; signed events remain separate.
    pub fallback_reads: u32,
}

impl Default for TaskLimits {
    fn default() -> Self {
        Self {
            model_nanos: 5_000_000_000,
            wall_seconds: 7 * 24 * 60 * 60,
            running_seconds: 2 * 60 * 60,
            leases: 12,
            publications: 8,
            decision_calls: 128,
            fallback_reads: 24,
        }
    }
}

/// Measured usage retained on the objective, including stopped attempts.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskUsage {
    /// Measured paid-model cost, including coding and decision calls.
    #[serde(with = "crate::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = crate::nanos::Decimal))]
    pub model_nanos: i64,
    /// Outstanding native model-call reservations.
    #[serde(with = "crate::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = crate::nanos::Decimal))]
    pub reserved_model_nanos: i64,
    /// Cumulative measured running time, in nanoseconds.
    pub running_nanos: u64,
    /// Distinct issued leases.
    pub leases: u32,
    /// Physical publication attempts.
    pub publications: u32,
    /// Physical decision calls, including retries.
    pub decision_calls: u32,
    /// Fallback dependency reads already consumed.
    pub fallback_reads: u32,
}
