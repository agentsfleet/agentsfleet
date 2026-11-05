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
    /// Immutable source item required for attributable task effects.
    #[serde(borrow)]
    pub originating_item_id: Cow<'a, str>,
    /// Exact target selected by native admission, never changed by recovered notes.
    pub target: super::TaskTarget,
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
    /// Last accepted input; recovery must incorporate any gap before new effects.
    pub input_sequence: u64,
    /// Native ordinal offered for the next immutable publication identity.
    pub effect_cursor: u64,
    /// Last confirmed effect covered by the saved state.
    pub confirmed_effect_watermark: u64,
    /// Compatible complete checkpoint, if one has been published.
    #[serde(borrow)]
    pub checkpoint_id: Option<Cow<'a, str>>,
    /// Last complete saved generation, distinct from allocated failed plans.
    pub checkpoint_generation: Option<u64>,
    /// Existing wall deadline; waking does not extend it.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
    /// Cumulative bounds, clamped by existing fleet/credit ceilings.
    pub limits: TaskLimits,
    /// Already consumed and reserved work.
    pub usage: TaskUsage,
}

impl TaskContext<'_> {
    /// Detach a decoded control response before dropping its HTTP body.
    #[must_use]
    pub fn into_owned(self) -> TaskContext<'static> {
        TaskContext {
            task_id: Cow::Owned(self.task_id.into_owned()),
            originating_item_id: Cow::Owned(self.originating_item_id.into_owned()),
            target: self.target,
            generation: self.generation,
            stop_generation: self.stop_generation,
            accepted_revision_id: Cow::Owned(self.accepted_revision_id.into_owned()),
            objective: Cow::Owned(self.objective.into_owned()),
            goal_digest: Cow::Owned(self.goal_digest.into_owned()),
            questions: self.questions,
            state: self.state,
            covered_input_sequence: self.covered_input_sequence,
            input_sequence: self.input_sequence,
            effect_cursor: self.effect_cursor,
            confirmed_effect_watermark: self.confirmed_effect_watermark,
            checkpoint_id: self.checkpoint_id.map(|id| Cow::Owned(id.into_owned())),
            checkpoint_generation: self.checkpoint_generation,
            expires_at: self.expires_at,
            limits: self.limits,
            usage: self.usage,
        }
    }
}
