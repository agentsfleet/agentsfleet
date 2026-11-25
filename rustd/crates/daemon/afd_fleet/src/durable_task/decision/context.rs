//! Semantic identity excludes worker ownership and changing accounting counters.

use af_core::id::Uuid7;
use af_wire::durable_task::{DecisionAssessment, DurableTaskContext};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub(super) struct DurableTaskStamp {
    pub durable_task_id: Uuid7,
    #[sqlx(try_from = "i64")]
    pub generation: u64,
    #[sqlx(try_from = "i64")]
    pub stop_generation: u64,
    pub goal_digest: String,
    pub state: String,
    #[sqlx(try_from = "i64")]
    pub covered_input_sequence: u64,
    #[sqlx(try_from = "i64")]
    pub effect_cursor: u64,
    #[sqlx(try_from = "i64")]
    pub confirmed_effect_watermark: u64,
}

impl DurableTaskStamp {
    /// The stamp `task` carries, or `None` when its identifiers are not
    /// `UUIDv7s` and so can match no stored stamp.
    pub(super) fn of(task: &DurableTaskContext<'_>) -> Option<Self> {
        Some(Self {
            durable_task_id: Uuid7::parse(&task.durable_task_id).ok()?,
            generation: task.generation,
            stop_generation: task.stop_generation,
            goal_digest: task.goal_digest.to_string(),
            state: task.state.as_str().to_owned(),
            covered_input_sequence: task.covered_input_sequence,
            effect_cursor: task.effect_cursor,
            confirmed_effect_watermark: task.confirmed_effect_watermark,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AssessedContext {
    pub source_turn_id: Uuid7,
    pub assessment: DecisionAssessment,
    pub profile_revision_id: Uuid7,
    pub questions_digest: String,
    pub execution: Option<DurableTaskStamp>,
}

#[derive(Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
enum Subject<'a> {
    Goal { action_group_id: &'a str },
}

impl AssessedContext {
    pub(super) fn subject(&self) -> Result<String> {
        subject(
            &self.assessment,
            self.execution
                .as_ref()
                .map(|task| (task.durable_task_id.as_str(), task.generation)),
        )
    }

    pub(super) fn key(&self) -> Result<String> {
        let DecisionAssessment::Goal { action_group_id } = &self.assessment;
        let subject = Subject::Goal { action_group_id };
        let bytes = serde_json::to_vec(&(
            subject,
            &self.profile_revision_id,
            &self.questions_digest,
            &self.execution,
        ))
        .map_err(af_core::json::Unreadable::from)?;
        Ok(format!(
            "{}/{}",
            self.subject()?,
            af_core::etag::sha256_hex(&bytes)
        ))
    }
}

pub(super) fn subject(
    assessment: &DecisionAssessment,
    task: Option<(&str, u64)>,
) -> Result<String> {
    let DecisionAssessment::Goal { action_group_id } = assessment;
    let (task, generation) = task.ok_or(crate::durable_task::DurableTaskRefusal::InvalidTarget)?;
    Ok(format!("task/{task}/{generation}/group/{action_group_id}"))
}
