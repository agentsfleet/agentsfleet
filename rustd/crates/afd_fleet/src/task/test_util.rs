//! Synthetic accepted-plan data for scenarios through the ordinary lifecycle.
//! These constructors neither validate targets nor change authorization paths.

use std::ops::Range;

use afd_core::id::Uuid7;
use afd_wire::{decision::NamedQuestion, task::TaskLimits};

use super::{PreparedPlan, PreparedTask};

impl PreparedTask {
    /// Assembles already-prepared fixture data; native bind guards still apply.
    #[must_use]
    pub fn from_parts(
        source_span: Range<i32>,
        title: String,
        objective: String,
        questions: Vec<NamedQuestion>,
        goal_digest: String,
        target_json: String,
        limits: TaskLimits,
    ) -> Self {
        Self {
            source_span,
            title,
            objective,
            questions,
            goal_digest,
            target_json,
            limits,
        }
    }
}

impl PreparedPlan {
    /// Assembles one complete fixture interpretation without exposing fields.
    #[must_use]
    pub fn from_parts(
        source_admission_id: Uuid7,
        accepted_revision_id: Uuid7,
        digest: String,
        items: Vec<PreparedTask>,
    ) -> Self {
        Self {
            source_admission_id,
            accepted_revision_id,
            digest,
            items: items.into_iter().map(|task| super::PreparedItem::New { interpretation: task.objective.clone(), task }).collect(),
            authorization: None,
        }
    }
}
