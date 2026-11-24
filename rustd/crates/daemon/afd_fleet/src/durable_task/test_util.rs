//! Synthetic plan data for scenarios through the ordinary lifecycle.
//! These constructors neither validate targets nor change authorization paths.

use std::ops::Range;

use af_core::id::Uuid7;
use af_wire::{decision::NamedQuestion, durable_task::DurableTaskLimits};

use super::{PreparedDurableTask, PreparedPlan};

impl PreparedDurableTask {
    /// Assembles already-prepared fixture data; native bind guards still apply.
    #[must_use]
    pub const fn from_parts(
        source_span: Range<i32>,
        title: String,
        objective: String,
        questions: Vec<NamedQuestion>,
        goal_digest: String,
        target_json: String,
        limits: DurableTaskLimits,
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
    /// Assembles one complete fixture plan of new tasks without exposing fields.
    #[must_use]
    pub fn from_parts(
        source_turn_id: Uuid7,
        digest: String,
        items: Vec<PreparedDurableTask>,
    ) -> Self {
        Self {
            source_turn_id,
            digest,
            items: items
                .into_iter()
                .map(|task| super::PreparedItem::New { task })
                .collect(),
        }
    }
}

impl super::DurableTasks {
    /// Makes `run` the first run of `task`'s generation, through the statement
    /// an issue attaches a continuation's run with. A suite that judges a
    /// task's goal holds a run bound to it without assigning the continuation.
    ///
    /// # Errors
    /// Datastore failures. `false` when the task cannot start another run.
    pub async fn attach_run(
        &self,
        run: &Uuid7,
        task: &Uuid7,
        generation: u64,
    ) -> crate::Result<bool> {
        let mut connection = self.runs.pool().acquire().await?;
        let generation = i64::try_from(generation).unwrap_or(i64::MAX);
        let deadline: Option<jiff_sqlx::Timestamp> =
            sqlx::query_scalar(include_str!("sql/issue_run.sql"))
                .bind(run)
                .bind(task)
                .bind(af_wire::durable_task::DurableTaskState::Ready.as_str())
                .bind(af_wire::durable_task::DurableTaskState::Running.as_str())
                .bind(generation)
                .bind(self.times().slice_seconds())
                .bind(super::run::NANOS_PER_SECOND)
                .fetch_optional(&mut *connection)
                .await
                .map_err(crate::error::query("attach a fixture run to its task"))?;
        Ok(deadline.is_some())
    }
}
