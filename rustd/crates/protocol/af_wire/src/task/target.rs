//! Native execution target, independent of any model's interpretation text.

use serde::{Deserialize, Serialize};

/// No repository is required for a repair performed only through APIs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskTarget {
    /// Repository-free work cannot acquire a coding checkout by implication.
    ApiOnly,
    /// Explicitly accepted existing same-repository pull request.
    ExistingPr {
        /// Canonical owner/name from GitHub, never a guessed repository.
        repository: String,
        /// Immutable native repository identity.
        repository_id: u64,
        /// Accepted pull request number.
        number: u32,
        /// Writable non-protected branch selected by native admission.
        branch: String,
        /// Accepted base branch.
        base: String,
        /// Latest native confirmed branch head.
        head: String,
        /// Tree at that head.
        tree: String,
        /// Confirmed pull request link.
        url: String,
        /// Owner-reviewed validation identities, never arbitrary workflow names.
        workflows: Vec<TaskWorkflow>,
    },
}

/// Frozen owner-reviewed validation identity.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWorkflow {
    /// Native GitHub workflow identity.
    pub id: u64,
    /// Owner-reviewed path.
    pub path: String,
    /// Owner-reviewed exact bytes.
    pub digest: String,
}
