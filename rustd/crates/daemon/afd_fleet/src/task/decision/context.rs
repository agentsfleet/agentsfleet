//! Semantic identity excludes worker ownership and changing accounting counters.

use af_core::id::Uuid7;
use af_wire::task::{DecisionAssessment, TaskContext};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub(super) struct TaskStamp {
    pub task_id: Uuid7,
    #[sqlx(try_from = "i64")]
    pub generation: u64,
    #[sqlx(try_from = "i64")]
    pub stop_generation: u64,
    pub accepted_revision_id: Uuid7,
    pub goal_digest: String,
    pub state: String,
    #[sqlx(try_from = "i64")]
    pub covered_input_sequence: u64,
    #[sqlx(try_from = "i64")]
    pub effect_cursor: u64,
    #[sqlx(try_from = "i64")]
    pub confirmed_effect_watermark: u64,
}

impl TaskStamp {
    /// The stamp `task` carries, or `None` when its identifiers are not
    /// `UUIDv7s` and so can match no stored stamp.
    pub(super) fn of(task: &TaskContext<'_>) -> Option<Self> {
        Some(Self {
            task_id: Uuid7::parse(&task.task_id).ok()?,
            generation: task.generation,
            stop_generation: task.stop_generation,
            accepted_revision_id: Uuid7::parse(&task.accepted_revision_id).ok()?,
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
    pub source_admission_id: Uuid7,
    pub assessment: DecisionAssessment,
    pub accepted_revision_id: Uuid7,
    pub profile_revision_id: Uuid7,
    pub questions_digest: String,
    pub execution: Option<TaskStamp>,
    pub candidates: Vec<TaskStamp>,
}

#[derive(Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
enum Subject<'a> {
    Message,
    Item { ordinal: u8 },
    Goal { action_group_id: &'a str },
}

impl AssessedContext {
    pub(super) fn subject(&self) -> Result<String> {
        subject(
            self.source_admission_id.as_str(),
            &self.assessment,
            self.execution
                .as_ref()
                .map(|task| (task.task_id.as_str(), task.generation)),
        )
    }

    pub(super) fn key(&self) -> Result<String> {
        let subject = match &self.assessment {
            DecisionAssessment::Message => Subject::Message,
            DecisionAssessment::Item { ordinal, .. } => Subject::Item { ordinal: *ordinal },
            DecisionAssessment::Goal { action_group_id } => Subject::Goal { action_group_id },
        };
        let source = (!matches!(self.assessment, DecisionAssessment::Goal { .. }))
            .then_some(&self.source_admission_id);
        let bytes = serde_json::to_vec(&(
            source,
            subject,
            &self.accepted_revision_id,
            &self.profile_revision_id,
            &self.questions_digest,
            &self.execution,
            &self.candidates,
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
    source: &str,
    assessment: &DecisionAssessment,
    task: Option<(&str, u64)>,
) -> Result<String> {
    match assessment {
        DecisionAssessment::Message => Ok(format!("source/{source}/message")),
        DecisionAssessment::Item { ordinal, .. } => Ok(format!("source/{source}/item/{ordinal}")),
        DecisionAssessment::Goal { action_group_id } => {
            let (task, generation) = task.ok_or(crate::task::TaskRefusal::InvalidTarget)?;
            Ok(format!("task/{task}/{generation}/group/{action_group_id}"))
        }
    }
}
