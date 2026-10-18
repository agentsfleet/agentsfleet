//! Renewal extends a live slot; it never revives an expired one.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Own-lease renewal with cumulative observations.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Renew {
    /// Decimal current fence.
    #[garde(length(bytes, min = 1, max = super::DECIMAL_BYTES_MAX))]
    pub fence: String,
    /// Monotone cumulative observations, verified by the daemon.
    #[garde(dive)]
    pub usage: super::Meter,
}

/// Live extension and remaining bounded output allowance.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Renewed {
    /// The lease's extended expiry.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
    /// Whether the runner must stop private work.
    pub stop_requested: bool,
    /// Output tokens still available across this attempt.
    pub remaining_output_tokens: u64,
    /// The original capability remains bound to the renewed lease.
    #[serde(with = "crate::instant")]
    pub model_access_expires_at: jiff::Timestamp,
}
