//! Public native wait observations, independent of model assessments.

use serde::{Deserialize, Serialize};

/// One reviewed workflow's last recorded exact-head observation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCheck {
    /// Reviewed GitHub workflow identity.
    pub workflow_id: u64,
    /// Observed workflow run, absent when none exists.
    pub run_id: Option<u64>,
    /// Observed workflow run number.
    pub run_number: Option<u64>,
    /// Observed attempt within the run.
    pub run_attempt: Option<u32>,
    /// Recorded provider status; absence of a run is pending.
    pub status: String,
    /// Recorded provider conclusion; absent until known.
    pub conclusion: Option<String>,
}

/// Current native dependency; reading it launches no remote checks.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDependency {
    /// Whether this observation still owns an unconsumed, uncancelled wait.
    pub active: bool,
    /// Exact branch head this wait observes.
    pub head: String,
    /// Saved observations of the reviewed workflows.
    pub checks: Vec<TaskCheck>,
    /// Time the native wait was updated; not a fresh provider observation time.
    #[serde(with = "crate::instant")]
    pub updated_at: jiff::Timestamp,
}
