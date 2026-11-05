//! Checked conversion of stored task state to its shared protocol context.

use std::borrow::Cow;

use afd_wire::task::{TaskContext, TaskLimits, TaskState, TaskUsage};
use serde::Deserialize;

use super::TaskSnapshot;
use crate::error::{Error, ErrorKind, Result};

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TaskRow {
    pub id: String,
    pub originating_item_id: String,
    pub target: String,
    pub accepted_revision_id: String,
    pub accepted_revision_number: i64,
    pub title: String,
    pub objective: String,
    pub questions: String,
    pub goal_digest: String,
    pub state: String,
    pub resource_version: i64,
    pub generation: i64,
    pub stop_generation: i64,
    pub covered_input_sequence: i64,
    pub input_sequence: i64,
    pub checkpoint_id: Option<String>,
    pub checkpoint_generation: Option<i64>,
    pub effect_cursor: i64,
    pub confirmed_effect_watermark: i64,
    pub max_model_nanos: i64,
    pub used_model_nanos: i64,
    pub reserved_model_nanos: i64,
    pub max_running_nanos: i64,
    pub used_running_nanos: i64,
    pub max_leases: i32,
    pub used_leases: i32,
    pub max_publications: i32,
    pub used_publications: i32,
    pub max_decision_calls: i32,
    pub used_decision_calls: i32,
    pub max_fallback_reads: i32,
    pub used_fallback_reads: i32,
    pub wall_seconds: i32,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
}

impl TaskRow {
    pub(super) fn snapshot(self) -> Result<TaskSnapshot> {
        let state = state(&self.state)?;
        let questions =
            serde_json::from_str(&self.questions).map_err(afd_core::json::Unreadable::from)?;
        Ok(TaskSnapshot {
            title: self.title,
            resource_version: counter(self.resource_version, "resource_version")?,
            accepted_revision_number: counter(self.accepted_revision_number, "revision")?,
            context: TaskContext {
                task_id: Cow::Owned(self.id),
                originating_item_id: Cow::Owned(self.originating_item_id),
                target: target(&self.target)?,
                generation: counter(self.generation, "generation")?,
                stop_generation: counter(self.stop_generation, "stop_generation")?,
                accepted_revision_id: Cow::Owned(self.accepted_revision_id),
                objective: Cow::Owned(self.objective),
                goal_digest: Cow::Owned(self.goal_digest),
                questions,
                state,
                covered_input_sequence: counter(
                    self.covered_input_sequence,
                    "covered_input_sequence",
                )?,
                input_sequence: counter(self.input_sequence, "input_sequence")?,
                effect_cursor: counter(self.effect_cursor, "effect_cursor")?,
                confirmed_effect_watermark: counter(
                    self.confirmed_effect_watermark,
                    "confirmed_effect_watermark",
                )?,
                checkpoint_id: self.checkpoint_id.map(Cow::Owned),
                checkpoint_generation: self.checkpoint_generation.map(|generation| counter(generation, "checkpoint generation")).transpose()?,
                expires_at: self.expires_at,
                limits: TaskLimits {
                    model_nanos: self.max_model_nanos,
                    wall_seconds: small_counter(self.wall_seconds, "wall_seconds")?,
                    running_seconds: small_counter(
                        self.max_running_nanos / 1_000_000_000,
                        "max_running_nanos",
                    )?,
                    leases: small_counter(self.max_leases, "max_leases")?,
                    publications: small_counter(self.max_publications, "max_publications")?,
                    decision_calls: small_counter(self.max_decision_calls, "max_decision_calls")?,
                    fallback_reads: small_counter(self.max_fallback_reads, "max_fallback_reads")?,
                },
                usage: TaskUsage {
                    model_nanos: self.used_model_nanos,
                    reserved_model_nanos: self.reserved_model_nanos,
                    running_nanos: counter(self.used_running_nanos, "used_running_nanos")?,
                    leases: small_counter(self.used_leases, "used_leases")?,
                    publications: small_counter(self.used_publications, "used_publications")?,
                    decision_calls: small_counter(self.used_decision_calls, "used_decision_calls")?,
                    fallback_reads: small_counter(self.used_fallback_reads, "used_fallback_reads")?,
                },
            },
        })
    }
}

fn target(encoded: &str) -> Result<afd_wire::task::TaskTarget> {
    let value: serde_json::Value = serde_json::from_str(encoded).map_err(afd_core::json::Unreadable::from)?;
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("api_only") { return Ok(afd_wire::task::TaskTarget::ApiOnly); }
    let target: super::github::BoundPr = serde_json::from_value(value).map_err(afd_core::json::Unreadable::from)?;
    Ok(afd_wire::task::TaskTarget::ExistingPr { repository: target.repository, repository_id: target.repository_id,
        number: target.number, branch: target.branch, base: target.base, head: target.head, tree: target.tree, url: target.url,
        workflows: target.workflows.into_iter().map(|workflow| afd_wire::task::TaskWorkflow {
            id: workflow.id, path: workflow.path, digest: workflow.digest }).collect() })
}

pub(super) fn state(stored: &str) -> Result<TaskState> {
    TaskState::deserialize(serde::de::value::StrDeserializer::new(stored))
        .map_err(|source: serde::de::value::Error| Error::from(ErrorKind::TaskState { source }))
}

pub(super) fn counter<T>(stored: T, column: &'static str) -> Result<u64>
where
    u64: TryFrom<T, Error = std::num::TryFromIntError>,
{
    u64::try_from(stored).map_err(|source| Error::from(ErrorKind::TaskCounter { column, source }))
}

pub(super) fn small_counter<T>(stored: T, column: &'static str) -> Result<u32>
where
    u32: TryFrom<T, Error = std::num::TryFromIntError>,
{
    u32::try_from(stored).map_err(|source| Error::from(ErrorKind::TaskCounter { column, source }))
}
