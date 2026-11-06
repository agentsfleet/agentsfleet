//! A task outcome is settled through the existing lease report.

use super::DecisionTaskVersion;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// Native goal disposition requested at a completed safe boundary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDisposition {
    /// All accepted questions and current native evidence support completion.
    Complete,
    /// A later exact-head validation event or bounded dependency read may resume work.
    Waiting,
    /// A prerequisite needs attention; no autonomous restart is authorized.
    Blocked,
    /// Save and fairly rejoin the existing admission order for another slice.
    Continue,
}

/// Exact native task state and its independently accounted goal assessment.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct TaskReport {
    /// Current task and generation at the last completed action group.
    #[garde(dive)]
    pub task: DecisionTaskVersion,
    /// Accepted semantic action; native code still verifies prerequisites.
    #[garde(skip)]
    pub disposition: TaskDisposition,
    /// Recorded native goal call, never a free-form model verdict.
    #[garde(inner(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    ))]
    pub decision_call_id: Option<String>,
    /// The compatible complete generation protecting the requested handoff.
    #[garde(inner(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    ))]
    pub checkpoint_id: Option<String>,
    /// Inputs actually incorporated in this run's conversation/file generation.
    #[garde(skip)]
    pub covered_input_sequence: u64,
    /// Confirmed effects incorporated locally, independently reconciled on restore.
    #[garde(skip)]
    pub effect_watermark: u64,
}
