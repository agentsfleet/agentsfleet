//! One selected objective's immutable goal and current native cursors.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::{TaskLimits, TaskState, TaskUsage};

/// Context for a task-bound lease; ordinary leases omit this owner.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskContext<'a> {
    /// Native task identity; never selected by a tool result.
    #[serde(borrow)]
    pub task_id: Cow<'a, str>,
    /// Generation the next semantic transition must assess.
    pub generation: u64,
    /// Committed cutoff for new effects.
    pub stop_generation: u64,
    /// Fleet revision whose reviewed goal was accepted.
    #[serde(borrow)]
    pub accepted_revision_id: Cow<'a, str>,
    /// Accepted objective, distinct from standing fleet instructions.
    #[serde(borrow)]
    pub objective: Cow<'a, str>,
    /// Digest of the frozen objective and completion questions.
    #[serde(borrow)]
    pub goal_digest: Cow<'a, str>,
    /// Accepted questions with stable identities and complete reviewed criteria.
    pub questions: Vec<crate::decision::NamedQuestion>,
    /// Current durable state.
    pub state: TaskState,
    /// Inputs whose meaning is included in the supplied state.
    pub covered_input_sequence: u64,
    /// Native ordinal offered for the next immutable publication identity.
    pub effect_cursor: u64,
    /// Last confirmed effect covered by the saved state.
    pub confirmed_effect_watermark: u64,
    /// Compatible complete checkpoint, if one has been published.
    #[serde(borrow)]
    pub checkpoint_id: Option<Cow<'a, str>>,
    /// Existing wall deadline; waking does not extend it.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
    /// Cumulative bounds, clamped by existing fleet/credit ceilings.
    pub limits: TaskLimits,
    /// Already consumed and reserved work.
    pub usage: TaskUsage,
}
