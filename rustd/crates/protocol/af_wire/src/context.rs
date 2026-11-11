//! Lease-scoped model-window checkpoints are private working state.
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Content-free state of one conversation summary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextState {
    /// One metered summary call is in progress.
    Summarizing,
    /// A validated checkpoint committed and preparation continues.
    Resuming,
    /// The operation stopped without a confirmed checkpoint.
    Interrupted,
}

/// Progress never contains notes, source messages or model-generated prose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextProgress {
    /// Monotonic checkpoint generation within this conversation branch.
    #[garde(range(min = 1, max = i64::MAX.unsigned_abs()))]
    pub generation: u64,
    /// Only confirmed publication may report resuming.
    #[garde(skip)]
    pub state: ContextState,
}
