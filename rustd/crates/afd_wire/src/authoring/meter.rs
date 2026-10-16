//! Cumulative runner observations cannot replace daemon provider receipts.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Whether all reported provider usage is known.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    /// All physical calls have authoritative usage.
    Known,
    /// At least one physical call has an unresolved obligation.
    Unknown,
}

/// Bounded cumulative observations supplied on renewal and terminal reports.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Meter {
    /// Unknown is never converted to known using a local estimate.
    #[garde(skip)]
    pub knowledge: Knowledge,
    /// Number of physical dispatches observed by the runner.
    #[garde(range(max = 8))]
    pub calls: u32,
    /// Known cumulative input tokens, when available.
    #[garde(inner(range(max = 262_144)))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Known cumulative output tokens, when available.
    #[garde(inner(range(max = 8192)))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Runtime from the attempt's monotonic timer.
    #[garde(range(max = 300_000))]
    pub runtime_ms: u64,
    /// Canonical decimal nanos, checked against daemon receipts.
    #[garde(inner(length(bytes, min = 1, max = super::DECIMAL_BYTES_MAX), custom(afd_validate::ascii_digits)))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_cost_nanos: Option<String>,
}

/// Creator-visible usage; unknown provider figures remain absent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Daemon-derived knowledge, independent of a runner estimate.
    pub knowledge: Knowledge,
    /// Cumulative input, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Cumulative output, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Physical provider cost, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_cost_nanos: Option<String>,
    /// Upward-rounded cumulative runtime cost.
    pub runtime_cost_nanos: String,
}
