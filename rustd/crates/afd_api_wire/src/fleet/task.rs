//! Task reads expose confirmed work and its accepted setup independently of chat.

use std::borrow::Cow;

use afd_wire::task::{TaskContext, TaskState};
use serde::{Deserialize, Serialize};

/// A bounded page of unfinished work belonging to one fleet.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TasksResponse<'a> {
    /// Newest work first; completed work remains available by its identity.
    #[serde(borrow)]
    pub items: Vec<TaskSummary<'a>>,
    /// Opaque keyset boundary, absent when no later work exists.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// Compact native state for the fleet's current-work panel.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSummary<'a> {
    /// Stable objective identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// User-facing work title.
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    /// Confirmed durable lifecycle state.
    pub status: TaskState,
    /// Monotonic native resource version.
    pub resource_version: u64,
    /// Current task ownership generation.
    pub generation: u64,
    /// Immutable fleet revision accepted by this work.
    #[serde(borrow)]
    pub accepted_revision_id: Cow<'a, str>,
    /// Native creation instant.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Existing cumulative deadline.
    #[serde(with = "afd_wire::instant")]
    pub expires_at: jiff::Timestamp,
}

/// Task detail with immutable success criteria and cumulative progress.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskResponse<'a> {
    /// User-facing title.
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    /// Version used by the strong resource tag.
    pub resource_version: u64,
    /// Human-readable number of the immutable setup accepted by this task.
    pub setup_revision: u64,
    /// Accepted objective, questions, cursors and remaining limits.
    #[serde(borrow)]
    pub context: TaskContext<'a>,
}
